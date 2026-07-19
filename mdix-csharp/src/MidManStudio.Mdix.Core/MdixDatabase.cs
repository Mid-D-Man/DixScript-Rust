using System;
using System.Collections.Generic;
using System.IO;
using System.Runtime.InteropServices;
using System.Text.Json;
using System.Threading;
using System.Threading.Tasks;
using MidManStudio.Mdix.Core.Internal;
using MidManStudio.DixScript.Native;

namespace MidManStudio.Mdix.Core
{
    #region MdixValueType

    /// <summary>
    /// Type discriminants for DixScript values.
    /// Values MUST match the Rust <c>MdixType</c> repr(i32) enum in mdix-ffi/src/lib.rs
    /// exactly — they cross the FFI boundary as raw integers.
    ///
    /// Numeric types are contiguous: Int=2, Long=3, Float=4, Double=5.
    /// </summary>
    public enum MdixValueType
    {
        Unknown   = -1,
        Null      =  0,
        Bool      =  1,
        Int       =  2,
        Long      =  3,   // 64-bit integer — directly under Int
        Float     =  4,
        Double    =  5,
        String    =  6,
        Date      =  7,
        Timestamp =  8,
        HexColor  =  9,
        Blob      = 10,
        Regex     = 11,
        Array     = 12,
        Object    = 13,
        Tuple     = 14,
        Enum      = 15,
    }

    #endregion

    public sealed unsafe partial class MdixDatabase : IDisposable
    {
        #region Fields

        private MdixSafeHandle  _safeHandle;
        private string?         _sourcePath;
        private volatile int    _disposed;

        private FileSystemWatcher? _watcher;
        private long               _lastReloadTick;
        private readonly object    _watcherLock = new object();

        private const long ReloadDebounceTicks = 5_000_000L;

        #endregion

        #region Events

        /// <summary>
        /// Fires after a successful hot reload. The <see cref="MdixDatabase"/>
        /// passed to the handler is always <c>this</c> same instance, mutated in
        /// place -- you don't need to (and shouldn't) replace whatever reference
        /// you're already holding.
        /// </summary>
        public event Action<MdixDatabase>? OnReloaded;
        public event Action<MdixError>? OnReloadFailed;

        #endregion

        #region Construction

        private MdixDatabase(void* rawHandle, string? sourcePath = null)
        {
            _safeHandle = new MdixSafeHandle(rawHandle);
            _sourcePath = sourcePath;
            _disposed   = 0;
        }

        #endregion

        #region IDisposable

        public void Dispose()
        {
            if (Interlocked.Exchange(ref _disposed, 1) != 0) return;
            DisableHotReload();
            _safeHandle.Dispose();
        }

        /// <summary>
        /// Used by operational methods (EnableHotReload, Deserialize, AsDynamic,
        /// Validate) that don't already return an MdixResult, where calling them
        /// on a disposed instance is a programmer error worth surfacing loudly.
        /// Value getters (GetString, GetInt, GetKeys, etc.) do NOT use this --
        /// they go through TryGetRawHandle instead, which folds the disposed
        /// check into the same MdixResult failure path as every other read
        /// error, so callers checking IsFailure don't get blindsided by an
        /// exception from what looks like every other Get* call. Both are
        /// intentional; keep new methods in the category that matches their
        /// existing return shape rather than picking whichever is more
        /// convenient in the moment.
        /// </summary>
        private void ThrowIfDisposed()
        {
            if (_disposed == 1)
                throw new ObjectDisposedException(
                    nameof(MdixDatabase),
                    "This MdixDatabase has been disposed.");
        }

        #endregion

        #region Properties

        public bool IsValid =>
            _disposed == 0 && !_safeHandle.IsInvalid && !_safeHandle.IsClosed;

        public int EntryCount
        {
            get
            {
                if (_disposed == 1) return -1;
                if (!TryGetRawHandle(out var h, out _)) return -1;
                try   { return MdixNative.mdix_entry_count(h); }
                finally { _safeHandle.DangerousRelease(); }
            }
        }

        #endregion

        #region Sync factories

        public static MdixResult<MdixDatabase> Load(string path)
        {
            if (string.IsNullOrEmpty(path))
                return MdixError.InvalidPath(path);

            fixed (byte* pathPtr = MdixStringCache.GetUtf8Bytes(path))
            {
                var handle = MdixNative.mdix_load(pathPtr);
                if (handle == null)
                    return MdixError.NativeError(
                        ReadLastError() ?? $"Failed to load '{path}'.");

                return MdixResult<MdixDatabase>.Ok(new MdixDatabase(handle, path));
            }
        }

        public static MdixResult<MdixDatabase> LoadStr(string source)
        {
            if (string.IsNullOrEmpty(source))
                return MdixError.ParseError("Source string is null or empty.");

            fixed (byte* srcPtr = MdixStringCache.GetUtf8Bytes(source))
            {
                var handle = MdixNative.mdix_load_str(srcPtr);
                if (handle == null)
                    return MdixError.NativeError(
                        ReadLastError() ?? "Failed to load from string source.");

                return MdixResult<MdixDatabase>.Ok(new MdixDatabase(handle));
            }
        }

        public static MdixResult<MdixDatabase> LoadEncrypted(
            string  encPath,
            string? keyPath = null)
        {
            if (string.IsNullOrEmpty(encPath))
                return MdixError.InvalidPath(encPath);

            fixed (byte* encPtr = MdixStringCache.GetUtf8Bytes(encPath))
            {
                void* handle;
                if (keyPath != null)
                {
                    fixed (byte* keyPtr = MdixStringCache.GetUtf8Bytes(keyPath))
                        handle = MdixNative.mdix_load_encrypted(encPtr, keyPtr);
                }
                else
                {
                    handle = MdixNative.mdix_load_encrypted(encPtr, null);
                }

                if (handle == null)
                    return MdixError.NativeError(
                        ReadLastError() ?? $"Failed to load encrypted file '{encPath}'.");

                return MdixResult<MdixDatabase>.Ok(new MdixDatabase(handle, encPath));
            }
        }

        public static MdixResult<MdixDatabase> LoadEncryptedPassword(
            string encPath,
            string password)
        {
            if (string.IsNullOrEmpty(encPath))  return MdixError.InvalidPath(encPath);
            if (string.IsNullOrEmpty(password)) return MdixError.NativeError("Password cannot be null or empty.");

            // EncodeTemporary's own contract says these bytes "should not persist
            // beyond the call site" -- assign to a local so we can actually zero
            // it afterward, rather than letting the fixed-pinned buffer just sit
            // in managed memory until GC eventually reclaims it.
            var pwdBytes = MdixStringCache.EncodeTemporary(password);
            try
            {
                fixed (byte* encPtr = MdixStringCache.GetUtf8Bytes(encPath))
                fixed (byte* pwdPtr = pwdBytes)
                {
                    var handle = MdixNative.mdix_load_encrypted_password(encPtr, pwdPtr);
                    if (handle == null)
                        return MdixError.NativeError(
                            ReadLastError() ?? $"Failed to load encrypted file '{encPath}'.");

                    return MdixResult<MdixDatabase>.Ok(new MdixDatabase(handle, encPath));
                }
            }
            finally
            {
                Array.Clear(pwdBytes, 0, pwdBytes.Length);
            }
        }

        public static MdixResult<MdixDatabase> LoadEncryptedBytes(
            byte[]  data,
            string  keyContent,
            string? password = null)
        {
            if (data == null || data.Length == 0) return MdixError.NativeError("Encrypted byte array is null or empty.");
            if (string.IsNullOrEmpty(keyContent)) return MdixError.NativeError("Key file content is null or empty.");

            byte[]? pwdBytes = password != null ? MdixStringCache.EncodeTemporary(password) : null;
            try
            {
                fixed (byte* dataPtr = data)
                fixed (byte* keyPtr  = MdixStringCache.GetUtf8Bytes(keyContent))
                {
                    void* handle;
                    if (pwdBytes != null)
                    {
                        fixed (byte* pwdPtr = pwdBytes)
                            handle = MdixNative.mdix_load_encrypted_bytes(
                                dataPtr, data.Length, keyPtr, pwdPtr);
                    }
                    else
                    {
                        handle = MdixNative.mdix_load_encrypted_bytes(
                            dataPtr, data.Length, keyPtr, null);
                    }

                    if (handle == null)
                        return MdixError.NativeError(
                            ReadLastError() ?? "Failed to load from encrypted bytes.");

                    return MdixResult<MdixDatabase>.Ok(new MdixDatabase(handle));
                }
            }
            finally
            {
                if (pwdBytes != null) Array.Clear(pwdBytes, 0, pwdBytes.Length);
            }
        }

        internal static MdixDatabase FromRawHandle(void* rawHandle) =>
            new MdixDatabase(rawHandle);

        #endregion

        #region Async factories

        public static Task<MdixResult<MdixDatabase>> LoadAsync(
            string path, CancellationToken ct = default) =>
            Task.Run(() => Load(path), ct);

        public static Task<MdixResult<MdixDatabase>> LoadStrAsync(
            string source, CancellationToken ct = default) =>
            Task.Run(() => LoadStr(source), ct);

        public static Task<MdixResult<MdixDatabase>> LoadEncryptedAsync(
            string encPath, string? keyPath = null, CancellationToken ct = default) =>
            Task.Run(() => LoadEncrypted(encPath, keyPath), ct);

        public static Task<MdixResult<MdixDatabase>> LoadEncryptedPasswordAsync(
            string encPath, string password, CancellationToken ct = default) =>
            Task.Run(() => LoadEncryptedPassword(encPath, password), ct);

        public static Task<MdixResult<MdixDatabase>> LoadEncryptedBytesAsync(
            byte[] data, string keyContent, string? password = null, CancellationToken ct = default) =>
            Task.Run(() => LoadEncryptedBytes(data, keyContent, password), ct);

        #endregion

        #region Data access — existence and type

        public bool Exists(string path)
        {
            if (_disposed == 1 || string.IsNullOrEmpty(path)) return false;
            if (!TryGetRawHandle(out var h, out _)) return false;
            try
            {
                fixed (byte* pathPtr = MdixStringCache.GetUtf8Bytes(path))
                    return MdixNative.mdix_exists(h, pathPtr);
            }
            finally { _safeHandle.DangerousRelease(); }
        }

        public MdixValueType GetValueType(string path)
        {
            if (_disposed == 1 || string.IsNullOrEmpty(path))
                return MdixValueType.Unknown;
            if (!TryGetRawHandle(out var h, out _))
                return MdixValueType.Unknown;
            try
            {
                fixed (byte* pathPtr = MdixStringCache.GetUtf8Bytes(path))
                    return (MdixValueType)(int)MdixNative.mdix_get_type(h, pathPtr);
            }
            finally { _safeHandle.DangerousRelease(); }
        }

        #endregion

        #region Data access — core typed getters

        public MdixResult<string> GetString(string path)
        {
            if (!TryGetRawHandleForPath(path, out var h, out var err)) return err;
            try
            {
                fixed (byte* pathPtr = MdixStringCache.GetUtf8Bytes(path))
                {
                    var ptr = MdixNative.mdix_get_string(h, pathPtr);
                    if (ptr == null)
                        return MdixError.NativeError(ReadLastError() ?? $"Path not found: '{path}'.");
                    return MdixResult<string>.Ok(ReadFreeNativeString(ptr)!);
                }
            }
            finally { _safeHandle.DangerousRelease(); }
        }

        public MdixResult<int> GetInt(string path)
        {
            if (!TryGetRawHandleForPath(path, out var h, out var err)) return err;
            try
            {
                fixed (byte* pathPtr = MdixStringCache.GetUtf8Bytes(path))
                {
                    MdixNative.mdix_clear_error();
                    int value = MdixNative.mdix_get_int(h, pathPtr);
                    var nativeErr = ReadLastError();
                    if (nativeErr != null) return MdixError.NativeError(nativeErr);
                    return MdixResult<int>.Ok(value);
                }
            }
            finally { _safeHandle.DangerousRelease(); }
        }

        /// <summary>
        /// Get a 64-bit integer. Use this for <see cref="MdixValueType.Long"/> values
        /// (DixScript <c>L</c>-suffixed literals or integers that overflow i32).
        /// Also accepts <see cref="MdixValueType.Int"/> values, widening without loss.
        /// </summary>
        public MdixResult<long> GetLong(string path)
        {
            if (!TryGetRawHandleForPath(path, out var h, out var err)) return err;
            try
            {
                fixed (byte* pathPtr = MdixStringCache.GetUtf8Bytes(path))
                {
                    MdixNative.mdix_clear_error();
                    long value = MdixNative.mdix_get_long(h, pathPtr);
                    var nativeErr = ReadLastError();
                    if (nativeErr != null) return MdixError.NativeError(nativeErr);
                    return MdixResult<long>.Ok(value);
                }
            }
            finally { _safeHandle.DangerousRelease(); }
        }

        public MdixResult<float> GetFloat(string path)
        {
            if (!TryGetRawHandleForPath(path, out var h, out var err)) return err;
            try
            {
                fixed (byte* pathPtr = MdixStringCache.GetUtf8Bytes(path))
                {
                    MdixNative.mdix_clear_error();
                    float value = MdixNative.mdix_get_float(h, pathPtr);
                    var nativeErr = ReadLastError();
                    if (nativeErr != null) return MdixError.NativeError(nativeErr);
                    return MdixResult<float>.Ok(value);
                }
            }
            finally { _safeHandle.DangerousRelease(); }
        }

        public MdixResult<double> GetDouble(string path)
        {
            if (!TryGetRawHandleForPath(path, out var h, out var err)) return err;
            try
            {
                fixed (byte* pathPtr = MdixStringCache.GetUtf8Bytes(path))
                {
                    MdixNative.mdix_clear_error();
                    double value = MdixNative.mdix_get_double(h, pathPtr);
                    var nativeErr = ReadLastError();
                    if (nativeErr != null) return MdixError.NativeError(nativeErr);
                    return MdixResult<double>.Ok(value);
                }
            }
            finally { _safeHandle.DangerousRelease(); }
        }

        public MdixResult<bool> GetBool(string path)
        {
            if (!TryGetRawHandleForPath(path, out var h, out var err)) return err;
            try
            {
                fixed (byte* pathPtr = MdixStringCache.GetUtf8Bytes(path))
                {
                    MdixNative.mdix_clear_error();
                    bool value = MdixNative.mdix_get_bool(h, pathPtr);
                    var nativeErr = ReadLastError();
                    if (nativeErr != null) return MdixError.NativeError(nativeErr);
                    return MdixResult<bool>.Ok(value);
                }
            }
            finally { _safeHandle.DangerousRelease(); }
        }

        public MdixResult<string> GetJson(string path)
        {
            if (!TryGetRawHandleForPath(path, out var h, out var err)) return err;
            try
            {
                fixed (byte* pathPtr = MdixStringCache.GetUtf8Bytes(path))
                {
                    var ptr = MdixNative.mdix_get_json(h, pathPtr);
                    if (ptr == null)
                        return MdixError.NativeError(ReadLastError() ?? $"Path not found: '{path}'.");
                    return MdixResult<string>.Ok(ReadFreeNativeString(ptr)!);
                }
            }
            finally { _safeHandle.DangerousRelease(); }
        }

        #endregion

        #region Data access — special types

        public MdixResult<MdixHexColor> GetHexColor(string path) =>
            GetString(path).AndThen(raw => MdixHexColor.Parse(raw));

        public MdixResult<MdixBlob> GetBlob(string path) =>
            GetString(path).Map(raw => new MdixBlob(raw));

        public MdixResult<MdixRegex> GetRegex(string path) =>
            GetString(path).Map(raw => new MdixRegex(raw));

        public MdixResult<MdixDate> GetDate(string path) =>
            GetString(path).AndThen(raw => MdixDate.Parse(raw));

        public MdixResult<MdixTimestamp> GetTimestamp(string path) =>
            GetString(path).AndThen(raw => MdixTimestamp.Parse(raw));

        #endregion

        #region Data access — enums

        public MdixResult<string> GetEnumName(string path)
        {
            if (!TryGetRawHandleForPath(path, out var h, out var err)) return err;
            try
            {
                fixed (byte* pathPtr = MdixStringCache.GetUtf8Bytes(path))
                {
                    var ptr = MdixNative.mdix_get_enum_name(h, pathPtr);
                    if (ptr == null)
                        return MdixError.NativeError(ReadLastError() ?? $"No enum name at '{path}'.");
                    return MdixResult<string>.Ok(ReadFreeNativeString(ptr)!);
                }
            }
            finally { _safeHandle.DangerousRelease(); }
        }

        public MdixResult<string> GetEnumField(string path)
        {
            if (!TryGetRawHandleForPath(path, out var h, out var err)) return err;
            try
            {
                fixed (byte* pathPtr = MdixStringCache.GetUtf8Bytes(path))
                {
                    var ptr = MdixNative.mdix_get_enum_field(h, pathPtr);
                    if (ptr == null)
                        return MdixError.NativeError(ReadLastError() ?? $"No enum field at '{path}'.");
                    return MdixResult<string>.Ok(ReadFreeNativeString(ptr)!);
                }
            }
            finally { _safeHandle.DangerousRelease(); }
        }

        public MdixResult<int> GetEnumValue(string path) => GetInt(path);

        #endregion

        #region Data access — arrays and keys

        public MdixResult<int> GetArrayLength(string path)
        {
            if (!TryGetRawHandleForPath(path, out var h, out var err)) return err;
            try
            {
                fixed (byte* pathPtr = MdixStringCache.GetUtf8Bytes(path))
                {
                    int length = MdixNative.mdix_get_array_length(h, pathPtr);
                    if (length < 0)
                        return MdixError.TypeMismatch(path, "array", GetValueType(path).ToString());
                    return MdixResult<int>.Ok(length);
                }
            }
            finally { _safeHandle.DangerousRelease(); }
        }

        public MdixResult<string[]> GetKeys(string? prefix = null)
        {
            if (!TryGetRawHandle(out var h, out var err)) return err;
            try
            {
                var prefixBytes = prefix != null
                    ? MdixStringCache.GetUtf8Bytes(prefix)
                    : MdixStringCache.GetUtf8Bytes(string.Empty);

                fixed (byte* prefixPtr = prefixBytes)
                {
                    int count;
                    var arr = MdixNative.mdix_get_keys(h, prefixPtr, &count);

                    if (arr == null || count <= 0)
                        return MdixResult<string[]>.Ok(Array.Empty<string>());

                    try
                    {
                        var result = new string[count];
                        for (int i = 0; i < count; i++)
                            result[i] = Marshal.PtrToStringUTF8((IntPtr)arr[i]) ?? string.Empty;
                        return MdixResult<string[]>.Ok(result);
                    }
                    finally
                    {
                        MdixNative.mdix_free_string_array(arr, count);
                    }
                }
            }
            finally { _safeHandle.DangerousRelease(); }
        }

        #endregion

        #region Data access — typed collections

        public MdixResult<List<T>> GetArray<T>(string path)
        {
            ThrowIfDisposed();

            if (string.IsNullOrEmpty(path))
                return MdixError.InvalidPath(path);

            var valueType = GetValueType(path);
            if (valueType == MdixValueType.Unknown)
                return MdixError.NotFound(path);
            if (valueType != MdixValueType.Array)
                return MdixError.TypeMismatch(path, "array", valueType.ToString());

            var lengthResult = GetArrayLength(path);
            if (lengthResult.IsFailure)
                return MdixResult<List<T>>.Err(lengthResult.Error);

            int count = lengthResult.SuccessResult;
            var list = new List<T>(count);
            var serializer = new MdixSerializer();

            for (int i = 0; i < count; i++)
            {
                var itemPath   = $"{path}[{i}]";
                var itemResult = GetSingleItem<T>(serializer, itemPath);
                if (itemResult.IsFailure)
                    return MdixResult<List<T>>.Err(itemResult.Error);
                list.Add(itemResult.SuccessResult);
            }

            return MdixResult<List<T>>.Ok(list);
        }

        public MdixResult<List<T>> GetAll<T>(string? prefix = null)
        {
            ThrowIfDisposed();

            if (!string.IsNullOrEmpty(prefix))
            {
                var vt = GetValueType(prefix);
                if (vt == MdixValueType.Array)
                    return GetArray<T>(prefix);
            }

            var keysResult = GetKeys(prefix);
            if (keysResult.IsFailure)
                return MdixResult<List<T>>.Err(keysResult.Error);

            var keys = keysResult.SuccessResult;
            if (keys.Length == 0)
                return MdixResult<List<T>>.Ok(new List<T>());

            var list       = new List<T>(keys.Length);
            var serializer = new MdixSerializer();

            foreach (var key in keys)
            {
                var fullPath   = string.IsNullOrEmpty(prefix) ? key : $"{prefix}.{key}";
                var itemResult = GetSingleItem<T>(serializer, fullPath);
                if (itemResult.IsFailure)
                    return MdixResult<List<T>>.Err(itemResult.Error);
                list.Add(itemResult.SuccessResult);
            }

            return MdixResult<List<T>>.Ok(list);
        }

        #endregion

        #region Data access — tuples

        public MdixResult<string>             GetTupleRaw(string path) => GetJson(path);

        public MdixResult<(T1, T2)>           GetTuple<T1, T2>(string path) =>
            GetJson(path).AndThen(json => ParseTuple<T1, T2>(json, path));

        public MdixResult<(T1, T2, T3)>       GetTuple<T1, T2, T3>(string path) =>
            GetJson(path).AndThen(json => ParseTuple<T1, T2, T3>(json, path));

        public MdixResult<(T1, T2, T3, T4)>   GetTuple<T1, T2, T3, T4>(string path) =>
            GetJson(path).AndThen(json => ParseTuple<T1, T2, T3, T4>(json, path));

        public MdixResult<(T1, T2, T3, T4, T5)> GetTuple<T1, T2, T3, T4, T5>(string path) =>
            GetJson(path).AndThen(json => ParseTuple<T1, T2, T3, T4, T5>(json, path));

        public MdixResult<(T1, T2, T3, T4, T5, T6)> GetTuple<T1, T2, T3, T4, T5, T6>(string path) =>
            GetJson(path).AndThen(json => ParseTuple<T1, T2, T3, T4, T5, T6>(json, path));

        #endregion

        #region Generic accessor

        public MdixResult<T> Get<T>(string path)
        {
            if (typeof(T) == typeof(string))        return CastResult<string,        T>(GetString(path));
            if (typeof(T) == typeof(int))           return CastResult<int,           T>(GetInt(path));
            if (typeof(T) == typeof(long))          return CastResult<long,          T>(GetLong(path));
            if (typeof(T) == typeof(float))         return CastResult<float,         T>(GetFloat(path));
            if (typeof(T) == typeof(double))        return CastResult<double,        T>(GetDouble(path));
            if (typeof(T) == typeof(bool))          return CastResult<bool,          T>(GetBool(path));
            if (typeof(T) == typeof(MdixHexColor))  return CastResult<MdixHexColor,  T>(GetHexColor(path));
            if (typeof(T) == typeof(MdixBlob))      return CastResult<MdixBlob,      T>(GetBlob(path));
            if (typeof(T) == typeof(MdixRegex))     return CastResult<MdixRegex,     T>(GetRegex(path));
            if (typeof(T) == typeof(MdixDate))      return CastResult<MdixDate,      T>(GetDate(path));
            if (typeof(T) == typeof(MdixTimestamp)) return CastResult<MdixTimestamp, T>(GetTimestamp(path));

            return MdixError.TypeMismatch(
                path, typeof(T).Name, "unsupported — use an explicit getter");
        }

        #endregion

        #region POCO deserialization

        public MdixResult<T> Deserialize<T>(string? prefix = null)
        {
            ThrowIfDisposed();
            var serializer = new MdixSerializer();
            return serializer.Deserialize<T>(this, prefix);
        }

        #endregion

        #region Dynamic access

        public MdixDynamic AsDynamic()
        {
            ThrowIfDisposed();
            return new MdixDynamic(this);
        }

        #endregion

        #region Schema validation

        public MdixValidationReport Validate(IMdixSchemaSource schema)
        {
            ThrowIfDisposed();
            if (schema is null) throw new ArgumentNullException(nameof(schema));
            return MdixDatabaseValidator.Validate(this, schema);
        }

        #endregion

        #region Hot reload

        public void EnableHotReload()
        {
            ThrowIfDisposed();

            if (_sourcePath == null)
                throw new InvalidOperationException(
                    "Hot reload requires a file path. Use Load(path) instead of LoadStr().");

            lock (_watcherLock)
            {
                if (_watcher != null) return;

                var dir  = Path.GetDirectoryName(Path.GetFullPath(_sourcePath)) ?? ".";
                var file = Path.GetFileName(_sourcePath);

                _watcher = new FileSystemWatcher(dir, file)
                {
                    NotifyFilter        = NotifyFilters.LastWrite | NotifyFilters.Size,
                    EnableRaisingEvents = true,
                };

                _watcher.Changed += HandleFileChanged;
                _watcher.Error   += HandleWatcherError;
            }
        }

        public void DisableHotReload()
        {
            lock (_watcherLock)
            {
                if (_watcher == null) return;
                _watcher.EnableRaisingEvents = false;
                _watcher.Changed -= HandleFileChanged;
                _watcher.Error   -= HandleWatcherError;
                _watcher.Dispose();
                _watcher = null;
            }
        }

        /// <summary>
        /// Reloads this database's data from its source file, in place. On
        /// success, <c>this</c> instance (the same reference you're already
        /// holding, and the same one <see cref="EnableHotReload"/> is watching)
        /// now reflects the new file contents -- it is also what
        /// <see cref="MdixResult{T}.SuccessResult"/> and <see cref="OnReloaded"/>
        /// hand back, so you never need to swap your own reference for a
        /// different object. If the file fails to load (missing, malformed,
        /// etc.), this instance's existing data is left completely untouched --
        /// a failed reload can never leave you holding a half-updated database.
        /// </summary>
        public MdixResult<MdixDatabase> Reload()
        {
            ThrowIfDisposed();
            if (_sourcePath == null)
                return MdixError.InvalidPath("No source path available for reload.");

            fixed (byte* pathPtr = MdixStringCache.GetUtf8Bytes(_sourcePath))
            {
                var newHandle = MdixNative.mdix_load(pathPtr);
                if (newHandle == null)
                    return MdixError.NativeError(
                        ReadLastError() ?? $"Failed to reload '{_sourcePath}'.");

                var oldSafeHandle = Interlocked.Exchange(ref _safeHandle, new MdixSafeHandle(newHandle));
                // Frees once any call already in flight against the old handle
                // (which acquired it via DangerousAddRef before this swap) finishes.
                oldSafeHandle.Dispose();

                if (_disposed == 1)
                {
                    // Dispose() raced in during the swap above and released the
                    // handle we just replaced *before* our new one was installed --
                    // it will never see this one, so we have to clean it up
                    // ourselves instead of leaking it.
                    _safeHandle.Dispose();
                    return MdixError.Disposed(nameof(MdixDatabase));
                }

                return MdixResult<MdixDatabase>.Ok(this);
            }
        }

        public Task<MdixResult<MdixDatabase>> ReloadAsync(CancellationToken ct = default) =>
            Task.Run(() => Reload(), ct);

        // HandleFileChanged / HandleWatcherError moved to the non-unsafe
        // partial class declaration at the bottom of this file — see the
        // comment there for why.

        #endregion

        #region Internal handle access — for MdixConverter

        internal bool TryGetRawHandleInternal(out void* rawHandle)
        {
            rawHandle = null;
            if (_disposed == 1) return false;

            bool acquired = false;
            _safeHandle.DangerousAddRef(ref acquired);

            if (!acquired || _safeHandle.IsInvalid)
            {
                if (acquired) _safeHandle.DangerousRelease();
                return false;
            }

            rawHandle = (void*)_safeHandle.DangerousGetHandle();
            return true;
        }

        internal void ReleaseRawHandleInternal() =>
            _safeHandle.DangerousRelease();

        #endregion

        #region Private helpers — handle acquisition

        private bool TryGetRawHandleForPath(
            string    path,
            out void* rawHandle,
            out MdixError error)
        {
            rawHandle = null;

            if (_disposed == 1)
            {
                error = MdixError.Disposed(nameof(MdixDatabase));
                return false;
            }

            if (string.IsNullOrEmpty(path))
            {
                error = MdixError.InvalidPath(path);
                return false;
            }

            return TryGetRawHandle(out rawHandle, out error);
        }

        private bool TryGetRawHandle(out void* rawHandle, out MdixError error)
        {
            rawHandle = null;

            if (_disposed == 1)
            {
                error = MdixError.Disposed(nameof(MdixDatabase));
                return false;
            }

            bool acquired = false;
            _safeHandle.DangerousAddRef(ref acquired);

            if (!acquired || _safeHandle.IsInvalid)
            {
                if (acquired) _safeHandle.DangerousRelease();
                error = MdixError.NullHandle();
                return false;
            }

            rawHandle = (void*)_safeHandle.DangerousGetHandle();
            error     = default;
            return true;
        }

        #endregion

        #region Private helpers — typed collection internals

        private MdixResult<T> GetSingleItem<T>(MdixSerializer serializer, string itemPath)
        {
            if (IsDirectlyGettable(typeof(T)))
                return Get<T>(itemPath);

            return serializer.Deserialize<T>(this, itemPath);
        }

        private static bool IsDirectlyGettable(Type t) =>
            t == typeof(string)       ||
            t == typeof(int)          ||
            t == typeof(long)         ||
            t == typeof(float)        ||
            t == typeof(double)       ||
            t == typeof(bool)         ||
            t == typeof(MdixHexColor) ||
            t == typeof(MdixBlob)     ||
            t == typeof(MdixRegex)    ||
            t == typeof(MdixDate)     ||
            t == typeof(MdixTimestamp);

        #endregion

        #region Private helpers — string marshalling

        private static string? ReadLastError()
        {
            var ptr = MdixNative.mdix_get_last_error();
            return ptr == null ? null : Marshal.PtrToStringUTF8((IntPtr)ptr);
        }

        private static string? ReadFreeNativeString(byte* ptr)
        {
            if (ptr == null) return null;
            try   { return Marshal.PtrToStringUTF8((IntPtr)ptr); }
            finally { MdixNative.mdix_free_string(ptr); }
        }

        #endregion

        #region Private helpers — generic cast

        private static MdixResult<TTarget> CastResult<TSource, TTarget>(
            MdixResult<TSource> source)
        {
            if (source.IsFailure) return MdixResult<TTarget>.Err(source.Error);
            return MdixResult<TTarget>.Ok((TTarget)(object)source.SuccessResult!);
        }

        #endregion

        #region Private helpers — tuple parsing

        private static MdixResult<T> ParseJsonElement<T>(JsonElement element, string path)
        {
            try
            {
                object? value;
                if      (typeof(T) == typeof(string))  value = element.ValueKind == JsonValueKind.String ? element.GetString() : element.ToString();
                else if (typeof(T) == typeof(int))     value = element.GetInt32();
                else if (typeof(T) == typeof(long))    value = element.GetInt64();
                else if (typeof(T) == typeof(float))   value = (float)element.GetDouble();
                else if (typeof(T) == typeof(double))  value = element.GetDouble();
                else if (typeof(T) == typeof(bool))    value = element.GetBoolean();
                else
                    return MdixError.TypeMismatch(path, typeof(T).Name, element.ValueKind.ToString());
                return MdixResult<T>.Ok((T)value!);
            }
            catch (Exception ex)
            {
                return MdixError.ParseError(
                    $"Failed to deserialize tuple element at '{path}': {ex.Message}");
            }
        }

        private static MdixResult<JsonElement[]> ParseJsonArray(
            string json, string path, int expectedLength)
        {
            try
            {
                var doc  = JsonDocument.Parse(json);
                var root = doc.RootElement;

                if (root.ValueKind != JsonValueKind.Array)
                    return MdixError.TypeMismatch(
                        path, $"tuple[{expectedLength}]", root.ValueKind.ToString());

                if (root.GetArrayLength() < expectedLength)
                    return MdixError.TypeMismatch(
                        path,
                        $"tuple with at least {expectedLength} elements",
                        $"array with {root.GetArrayLength()} elements");

                var elements = new JsonElement[expectedLength];
                for (int i = 0; i < expectedLength; i++)
                    elements[i] = root[i].Clone();

                doc.Dispose();
                return MdixResult<JsonElement[]>.Ok(elements);
            }
            catch (JsonException ex)
            {
                return MdixError.ParseError($"Invalid tuple JSON at '{path}': {ex.Message}");
            }
        }

        private static MdixResult<(T1, T2)> ParseTuple<T1, T2>(string json, string path)
        {
            var arr = ParseJsonArray(json, path, 2);
            if (arr.IsFailure) return MdixResult<(T1, T2)>.Err(arr.Error);
            var e = arr.SuccessResult;
            var v1 = ParseJsonElement<T1>(e[0], path); if (v1.IsFailure) return MdixResult<(T1, T2)>.Err(v1.Error);
            var v2 = ParseJsonElement<T2>(e[1], path); if (v2.IsFailure) return MdixResult<(T1, T2)>.Err(v2.Error);
            return MdixResult<(T1, T2)>.Ok((v1.SuccessResult, v2.SuccessResult));
        }

        private static MdixResult<(T1, T2, T3)> ParseTuple<T1, T2, T3>(string json, string path)
        {
            var arr = ParseJsonArray(json, path, 3);
            if (arr.IsFailure) return MdixResult<(T1, T2, T3)>.Err(arr.Error);
            var e = arr.SuccessResult;
            var v1 = ParseJsonElement<T1>(e[0], path); if (v1.IsFailure) return MdixResult<(T1, T2, T3)>.Err(v1.Error);
            var v2 = ParseJsonElement<T2>(e[1], path); if (v2.IsFailure) return MdixResult<(T1, T2, T3)>.Err(v2.Error);
            var v3 = ParseJsonElement<T3>(e[2], path); if (v3.IsFailure) return MdixResult<(T1, T2, T3)>.Err(v3.Error);
            return MdixResult<(T1, T2, T3)>.Ok((v1.SuccessResult, v2.SuccessResult, v3.SuccessResult));
        }

        private static MdixResult<(T1, T2, T3, T4)> ParseTuple<T1, T2, T3, T4>(string json, string path)
        {
            var arr = ParseJsonArray(json, path, 4);
            if (arr.IsFailure) return MdixResult<(T1, T2, T3, T4)>.Err(arr.Error);
            var e = arr.SuccessResult;
            var v1 = ParseJsonElement<T1>(e[0], path); if (v1.IsFailure) return MdixResult<(T1, T2, T3, T4)>.Err(v1.Error);
            var v2 = ParseJsonElement<T2>(e[1], path); if (v2.IsFailure) return MdixResult<(T1, T2, T3, T4)>.Err(v2.Error);
            var v3 = ParseJsonElement<T3>(e[2], path); if (v3.IsFailure) return MdixResult<(T1, T2, T3, T4)>.Err(v3.Error);
            var v4 = ParseJsonElement<T4>(e[3], path); if (v4.IsFailure) return MdixResult<(T1, T2, T3, T4)>.Err(v4.Error);
            return MdixResult<(T1, T2, T3, T4)>.Ok((v1.SuccessResult, v2.SuccessResult, v3.SuccessResult, v4.SuccessResult));
        }

        private static MdixResult<(T1, T2, T3, T4, T5)> ParseTuple<T1, T2, T3, T4, T5>(string json, string path)
        {
            var arr = ParseJsonArray(json, path, 5);
            if (arr.IsFailure) return MdixResult<(T1, T2, T3, T4, T5)>.Err(arr.Error);
            var e = arr.SuccessResult;
            var v1 = ParseJsonElement<T1>(e[0], path); if (v1.IsFailure) return MdixResult<(T1, T2, T3, T4, T5)>.Err(v1.Error);
            var v2 = ParseJsonElement<T2>(e[1], path); if (v2.IsFailure) return MdixResult<(T1, T2, T3, T4, T5)>.Err(v2.Error);
            var v3 = ParseJsonElement<T3>(e[2], path); if (v3.IsFailure) return MdixResult<(T1, T2, T3, T4, T5)>.Err(v3.Error);
            var v4 = ParseJsonElement<T4>(e[3], path); if (v4.IsFailure) return MdixResult<(T1, T2, T3, T4, T5)>.Err(v4.Error);
            var v5 = ParseJsonElement<T5>(e[4], path); if (v5.IsFailure) return MdixResult<(T1, T2, T3, T4, T5)>.Err(v5.Error);
            return MdixResult<(T1, T2, T3, T4, T5)>.Ok((v1.SuccessResult, v2.SuccessResult, v3.SuccessResult, v4.SuccessResult, v5.SuccessResult));
        }

        private static MdixResult<(T1, T2, T3, T4, T5, T6)> ParseTuple<T1, T2, T3, T4, T5, T6>(string json, string path)
        {
            var arr = ParseJsonArray(json, path, 6);
            if (arr.IsFailure) return MdixResult<(T1, T2, T3, T4, T5, T6)>.Err(arr.Error);
            var e = arr.SuccessResult;
            var v1 = ParseJsonElement<T1>(e[0], path); if (v1.IsFailure) return MdixResult<(T1, T2, T3, T4, T5, T6)>.Err(v1.Error);
            var v2 = ParseJsonElement<T2>(e[1], path); if (v2.IsFailure) return MdixResult<(T1, T2, T3, T4, T5, T6)>.Err(v2.Error);
            var v3 = ParseJsonElement<T3>(e[2], path); if (v3.IsFailure) return MdixResult<(T1, T2, T3, T4, T5, T6)>.Err(v3.Error);
            var v4 = ParseJsonElement<T4>(e[3], path); if (v4.IsFailure) return MdixResult<(T1, T2, T3, T4, T5, T6)>.Err(v4.Error);
            var v5 = ParseJsonElement<T5>(e[4], path); if (v5.IsFailure) return MdixResult<(T1, T2, T3, T4, T5, T6)>.Err(v5.Error);
            var v6 = ParseJsonElement<T6>(e[5], path); if (v6.IsFailure) return MdixResult<(T1, T2, T3, T4, T5, T6)>.Err(v6.Error);
            return MdixResult<(T1, T2, T3, T4, T5, T6)>.Ok((v1.SuccessResult, v2.SuccessResult, v3.SuccessResult, v4.SuccessResult, v5.SuccessResult, v6.SuccessResult));
        }

        #endregion
    }

    // Second partial declaration, deliberately NOT `unsafe`.
    //
    // BUGFIX (CS4004 "Cannot await in an unsafe context"): the main
    // MdixDatabase declaration above is `unsafe class MdixDatabase` because
    // most of its methods use `fixed`/`void*` to marshal UTF-8 buffers
    // across the native FFI boundary. In C#, `unsafe` on a type declaration
    // makes *every* member textually inside that declaration an unsafe
    // context -- including HandleFileChanged, which never touches a pointer
    // itself but used `await Task.Delay(...)`, and the compiler rejects
    // `await` anywhere inside an unsafe context regardless of whether that
    // specific method is the one using pointers.
    //
    // `unsafe` on a partial class only applies to the code within that
    // specific partial declaration, not to the combined type -- so moving
    // these two handlers into a second, plain (non-unsafe) `partial class
    // MdixDatabase` block resolves it without scoping `unsafe` down
    // per-method across the ~25 other members that genuinely need it.
    // Private fields/methods/events declared in either part (`_disposed`,
    // `_lastReloadTick`, `ReloadDebounceTicks`, `Reload()`, `OnReloaded`,
    // `OnReloadFailed`) are visible from both, since partial declarations
    // are the same type at compile time.
    public sealed partial class MdixDatabase
    {
        private async void HandleFileChanged(object sender, FileSystemEventArgs e)
        {
            try
            {
                // Lock-free debounce via compare-and-swap: only the thread that
                // successfully wins the exchange proceeds. The previous
                // read-then-separately-exchange version had a window where two
                // near-simultaneous FileSystemWatcher events (which genuinely
                // happens -- FSW is known to fire more than once per logical
                // save) could both pass the check before either updated the
                // tick, triggering two reloads instead of one.
                long last;
                long now;
                do
                {
                    now  = DateTime.UtcNow.Ticks;
                    last = Interlocked.Read(ref _lastReloadTick);
                    if (now - last < ReloadDebounceTicks) return;
                }
                while (Interlocked.CompareExchange(ref _lastReloadTick, now, last) != last);

                // Give the writer a moment to finish flushing before we read --
                // Task.Delay instead of Thread.Sleep so this doesn't tie up a
                // thread-pool thread while it waits.
                await Task.Delay(100).ConfigureAwait(false);

                if (_disposed == 1) return; // disposed while we were waiting

                var result = Reload();
                if (result.IsSuccess) OnReloaded?.Invoke(result.SuccessResult);
                else                  OnReloadFailed?.Invoke(result.Error);
            }
            catch (Exception ex)
            {
                // This is an async void event handler -- an exception escaping
                // it would crash the process instead of being catchable by
                // anyone. Route it through the normal failure event instead.
                OnReloadFailed?.Invoke(MdixError.IoError($"Hot reload handler threw: {ex.Message}", ex));
            }
        }

        private void HandleWatcherError(object sender, ErrorEventArgs e) =>
            OnReloadFailed?.Invoke(
                MdixError.IoError(
                    $"File watcher error: {e.GetException()?.Message}",
                    e.GetException()));
    }
}
