using System;
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
    /// Public-facing type discriminant returned by <see cref="MdixDatabase.GetValueType"/>.
    /// Maps 1:1 to the native MdixType enum.
    /// </summary>
    public enum MdixValueType
    {
        Unknown   = -1,
        Null      =  0,
        Bool      =  1,
        Int       =  2,
        Float     =  3,
        Double    =  4,
        String    =  5,
        Date      =  6,
        Timestamp =  7,
        HexColor  =  8,
        Blob      =  9,
        Regex     = 10,
        Array     = 11,
        Object    = 12,
        Tuple     = 13,
        Enum      = 14,
    }

    #endregion

    /// <summary>
    /// Loaded DixScript data container. All read operations are O(1) via the
    /// Rust-side flattened hash map. Implements <see cref="IDisposable"/> —
    /// always dispose when done to release the underlying native handle.
    /// </summary>
    public sealed unsafe class MdixDatabase : IDisposable
    {
        #region Fields

        private MdixSafeHandle  _safeHandle;
        private string?         _sourcePath;
        private volatile int    _disposed;

        private FileSystemWatcher? _watcher;
        private long               _lastReloadTick;
        private readonly object    _watcherLock = new object();

        // 500 ms debounce — editors often flush files in multiple writes.
        private const long ReloadDebounceTicks = 5_000_000L;

        #endregion

        #region Events

        /// <summary>
        /// Fired on the watcher background thread after a successful hot reload.
        /// The argument is the newly loaded <see cref="MdixDatabase"/> — the old
        /// instance remains valid until you dispose it.
        /// </summary>
        public event Action<MdixDatabase>? OnReloaded;

        /// <summary>Fired on the watcher background thread when a hot reload fails.</summary>
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

        /// <summary>
        /// Releases the native handle and stops hot reload.
        /// Safe to call multiple times.
        /// </summary>
        public void Dispose()
        {
            if (Interlocked.Exchange(ref _disposed, 1) != 0) return;
            DisableHotReload();
            _safeHandle.Dispose();
        }

        private void ThrowIfDisposed()
        {
            if (_disposed == 1)
                throw new ObjectDisposedException(
                    nameof(MdixDatabase),
                    "This MdixDatabase has been disposed.");
        }

        #endregion

        #region Properties

        /// <summary>True when the handle is valid and the database is not disposed.</summary>
        public bool IsValid =>
            _disposed == 0 && !_safeHandle.IsInvalid && !_safeHandle.IsClosed;

        /// <summary>
        /// Total number of entries in the flattened store, including indexed array elements.
        /// Returns -1 when disposed.
        /// </summary>
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

        /// <summary>Loads a plain .mdix file from disk.</summary>
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

        /// <summary>
        /// Loads DixScript source directly from a string — no disk access.
        /// Hot reload is unavailable for string-loaded instances.
        /// </summary>
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

        /// <summary>
        /// Loads an encrypted .mdix.enc file using a key file.
        /// Pass null for keyPath to auto-detect the key file next to the enc file.
        /// </summary>
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

        /// <summary>
        /// Loads an encrypted .mdix.enc file using a password.
        /// The password is encoded transiently and never stored in the string cache.
        /// </summary>
        public static MdixResult<MdixDatabase> LoadEncryptedPassword(
            string encPath,
            string password)
        {
            if (string.IsNullOrEmpty(encPath))  return MdixError.InvalidPath(encPath);
            if (string.IsNullOrEmpty(password)) return MdixError.NativeError("Password cannot be null or empty.");

            fixed (byte* encPtr = MdixStringCache.GetUtf8Bytes(encPath))
            fixed (byte* pwdPtr = MdixStringCache.EncodeTemporary(password))
            {
                var handle = MdixNative.mdix_load_encrypted_password(encPtr, pwdPtr);
                if (handle == null)
                    return MdixError.NativeError(
                        ReadLastError() ?? $"Failed to load encrypted file '{encPath}'.");

                return MdixResult<MdixDatabase>.Ok(new MdixDatabase(handle, encPath));
            }
        }

        /// <summary>
        /// Loads encrypted data from raw bytes with the key file content as a string.
        /// Useful when payload and key both come from a network response or secrets manager.
        /// </summary>
        public static MdixResult<MdixDatabase> LoadEncryptedBytes(
            byte[]  data,
            string  keyContent,
            string? password = null)
        {
            if (data == null || data.Length == 0) return MdixError.NativeError("Encrypted byte array is null or empty.");
            if (string.IsNullOrEmpty(keyContent)) return MdixError.NativeError("Key file content is null or empty.");

            fixed (byte* dataPtr = data)
            fixed (byte* keyPtr  = MdixStringCache.GetUtf8Bytes(keyContent))
            {
                void* handle;

                if (password != null)
                {
                    fixed (byte* pwdPtr = MdixStringCache.EncodeTemporary(password))
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

        #endregion

        #region Async factories

        /// <summary>Loads a plain .mdix file on a background thread.</summary>
        public static Task<MdixResult<MdixDatabase>> LoadAsync(
            string            path,
            CancellationToken ct = default) =>
            Task.Run(() => Load(path), ct);

        /// <summary>Loads DixScript source from a string on a background thread.</summary>
        public static Task<MdixResult<MdixDatabase>> LoadStrAsync(
            string            source,
            CancellationToken ct = default) =>
            Task.Run(() => LoadStr(source), ct);

        /// <summary>Loads an encrypted file on a background thread.</summary>
        public static Task<MdixResult<MdixDatabase>> LoadEncryptedAsync(
            string            encPath,
            string?           keyPath = null,
            CancellationToken ct      = default) =>
            Task.Run(() => LoadEncrypted(encPath, keyPath), ct);

        /// <summary>Loads an encrypted file using a password on a background thread.</summary>
        public static Task<MdixResult<MdixDatabase>> LoadEncryptedPasswordAsync(
            string            encPath,
            string            password,
            CancellationToken ct = default) =>
            Task.Run(() => LoadEncryptedPassword(encPath, password), ct);

        /// <summary>Loads encrypted bytes on a background thread.</summary>
        public static Task<MdixResult<MdixDatabase>> LoadEncryptedBytesAsync(
            byte[]            data,
            string            keyContent,
            string?           password = null,
            CancellationToken ct       = default) =>
            Task.Run(() => LoadEncryptedBytes(data, keyContent, password), ct);

        #endregion

        #region Data access — existence and type

        /// <summary>Returns true if the dotted path exists in the loaded data.</summary>
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

        /// <summary>
        /// Returns the <see cref="MdixValueType"/> of the value at path.
        /// Returns <see cref="MdixValueType.Unknown"/> when the path does not exist.
        /// </summary>
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

        /// <summary>
        /// Gets a string value by dotted path.
        /// Also works for Date, Timestamp, and HexColor — all stored as strings at the native layer.
        /// </summary>
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

        /// <summary>
        /// Gets an integer value by dotted path.
        /// Also resolves Enum values — returns the integer (e.g. BOSS → 2).
        /// Returns 0 on failure — use <see cref="Exists"/> to distinguish 0 from not-found.
        /// </summary>
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

        /// <summary>Gets a float value by dotted path.</summary>
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

        /// <summary>Gets a double value by dotted path.</summary>
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

        /// <summary>Gets a boolean value by dotted path.</summary>
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

        /// <summary>
        /// Gets the raw JSON representation of the value at path.
        /// Useful as an escape hatch for objects, arrays, tuples, and blobs.
        /// </summary>
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

        /// <summary>Gets and parses a hex color value (e.g. #FF5733) at path.</summary>
        public MdixResult<MdixHexColor> GetHexColor(string path) =>
            GetString(path).AndThen(raw => MdixHexColor.Parse(raw));

        /// <summary>
        /// Gets a blob value at path.
        /// Call <see cref="MdixBlob.ToBytes"/> to decode.
        /// </summary>
        public MdixResult<MdixBlob> GetBlob(string path) =>
            GetString(path).Map(raw => new MdixBlob(raw));

        /// <summary>
        /// Gets a regex value at path.
        /// Call <see cref="MdixRegex.ToRegex"/> or <see cref="MdixRegex.IsMatch"/> to use it.
        /// </summary>
        public MdixResult<MdixRegex> GetRegex(string path) =>
            GetString(path).Map(raw => new MdixRegex(raw));

        /// <summary>Gets and parses a date value (YYYY-MM-DD) at path.</summary>
        public MdixResult<MdixDate> GetDate(string path) =>
            GetString(path).AndThen(raw => MdixDate.Parse(raw));

        /// <summary>Gets and parses an ISO 8601 timestamp at path.</summary>
        public MdixResult<MdixTimestamp> GetTimestamp(string path) =>
            GetString(path).AndThen(raw => MdixTimestamp.Parse(raw));

        #endregion

        #region Data access — enums

        /// <summary>Returns the enum type name at path (e.g. "AIType").</summary>
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

        /// <summary>Returns the enum field name at path (e.g. "BOSS").</summary>
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

        /// <summary>Returns the resolved integer value of the enum at path (e.g. BOSS → 2).</summary>
        public MdixResult<int> GetEnumValue(string path) => GetInt(path);

        #endregion

        #region Data access — arrays and keys

        /// <summary>
        /// Returns the number of items in the array at path.
        /// Returns -1 if the path does not exist or is not an array.
        /// </summary>
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

        /// <summary>
        /// Returns the direct child key names under prefix.
        /// Pass an empty string or null to get top-level keys.
        /// </summary>
        public MdixResult<string[]> GetKeys(string? prefix = null)
        {
            ThrowIfDisposed();
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

        #region Data access — tuples

        /// <summary>
        /// Returns the raw JSON string for a tuple at path (e.g. [1,"hello",true]).
        /// Parse with your preferred JSON library.
        /// </summary>
        public MdixResult<string> GetTupleRaw(string path) => GetJson(path);

        /// <summary>Gets a 2-element tuple at path.</summary>
        public MdixResult<(T1, T2)> GetTuple<T1, T2>(string path) =>
            GetJson(path).AndThen(json => ParseTuple<T1, T2>(json, path));

        /// <summary>Gets a 3-element tuple at path.</summary>
        public MdixResult<(T1, T2, T3)> GetTuple<T1, T2, T3>(string path) =>
            GetJson(path).AndThen(json => ParseTuple<T1, T2, T3>(json, path));

        /// <summary>Gets a 4-element tuple at path.</summary>
        public MdixResult<(T1, T2, T3, T4)> GetTuple<T1, T2, T3, T4>(string path) =>
            GetJson(path).AndThen(json => ParseTuple<T1, T2, T3, T4>(json, path));

        /// <summary>Gets a 5-element tuple at path.</summary>
        public MdixResult<(T1, T2, T3, T4, T5)> GetTuple<T1, T2, T3, T4, T5>(string path) =>
            GetJson(path).AndThen(json => ParseTuple<T1, T2, T3, T4, T5>(json, path));

        /// <summary>Gets a 6-element tuple at path.</summary>
        public MdixResult<(T1, T2, T3, T4, T5, T6)> GetTuple<T1, T2, T3, T4, T5, T6>(string path) =>
            GetJson(path).AndThen(json => ParseTuple<T1, T2, T3, T4, T5, T6>(json, path));

        #endregion

        #region Generic accessor

        /// <summary>
        /// Dispatches to the appropriate typed getter based on T.
        /// Supports: string, int, float, double, bool, MdixHexColor, MdixBlob,
        /// MdixRegex, MdixDate, MdixTimestamp.
        /// </summary>
        public MdixResult<T> Get<T>(string path)
        {
            if (typeof(T) == typeof(string))        return CastResult<string,        T>(GetString(path));
            if (typeof(T) == typeof(int))           return CastResult<int,           T>(GetInt(path));
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

        /// <summary>
        /// Deserializes the database into a strongly-typed object.
        /// Supports classes, structs, and records including those with primary constructors.
        /// <para>
        /// Path resolution order for each property:
        /// 1. Explicit <see cref="MdixPropertyAttribute"/> path.
        /// 2. <see cref="MdixAliasAttribute"/> fallback paths (tried in declaration order).
        /// 3. Property name converted to snake_case automatically.
        /// </para>
        /// </summary>
        /// <typeparam name="T">Target type. Must have a parameterless or mappable parameterized constructor.</typeparam>
        /// <param name="prefix">
        /// Root path prefix prepended to every property path.
        /// Overrides any <see cref="MdixObjectAttribute"/> declared on <typeparamref name="T"/>.
        /// Pass null to use the attribute value, or an empty string to read from the root.
        /// </param>
        public MdixResult<T> Deserialize<T>(string? prefix = null)
        {
            ThrowIfDisposed();
            var serializer = new MdixSerializer();
            return serializer.Deserialize<T>(this, prefix);
        }

        #endregion

        #region Dynamic access

        /// <summary>
        /// Returns a <see cref="MdixDynamic"/> wrapper over this database for
        /// path navigation without string literals at call sites.
        /// <code>
        /// dynamic cfg = db.AsDynamic();
        /// int port    = cfg.server.port;
        /// string name = cfg.enemies[0].name;
        /// </code>
        /// </summary>
        public MdixDynamic AsDynamic()
        {
            ThrowIfDisposed();
            return new MdixDynamic(this);
        }

        #endregion

        #region Schema validation

        /// <summary>
        /// Validates this database against the given schema.
        /// All errors are collected in a single pass — never throws.
        /// </summary>
        public MdixValidationReport Validate(IMdixSchemaSource schema)
        {
            ThrowIfDisposed();
            if (schema is null) throw new ArgumentNullException(nameof(schema));
            return MdixDatabaseValidator.Validate(this, schema);
        }

        #endregion

        #region Hot reload

        /// <summary>
        /// Starts watching the source file for changes and reloads automatically.
        /// <see cref="OnReloaded"/> and <see cref="OnReloadFailed"/> are fired on a
        /// background thread.
        /// </summary>
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

        /// <summary>Stops the file watcher. Safe to call even when hot reload is not active.</summary>
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
        /// Manually reloads from the source file. Returns a new
        /// <see cref="MdixDatabase"/> on success — the current instance
        /// is unaffected and must be disposed separately.
        /// </summary>
        public MdixResult<MdixDatabase> Reload()
        {
            ThrowIfDisposed();
            if (_sourcePath == null)
                return MdixError.InvalidPath("No source path available for reload.");
            return Load(_sourcePath);
        }

        /// <summary>Reloads on a background thread.</summary>
        public Task<MdixResult<MdixDatabase>> ReloadAsync(CancellationToken ct = default) =>
            Task.Run(() => Reload(), ct);

        private void HandleFileChanged(object sender, FileSystemEventArgs e)
        {
            var now  = DateTime.UtcNow.Ticks;
            var last = Interlocked.Read(ref _lastReloadTick);
            if (now - last < ReloadDebounceTicks) return;
            Interlocked.Exchange(ref _lastReloadTick, now);

            Thread.Sleep(100);

            var result = Reload();
            if (result.IsSuccess) OnReloaded?.Invoke(result.SuccessResult);
            else                  OnReloadFailed?.Invoke(result.Error);
        }

        private void HandleWatcherError(object sender, ErrorEventArgs e) =>
            OnReloadFailed?.Invoke(
                MdixError.IoError(
                    $"File watcher error: {e.GetException()?.Message}",
                    e.GetException()));

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
                else if (typeof(T) == typeof(float))   value = (float)element.GetDouble();
                else if (typeof(T) == typeof(double))  value = element.GetDouble();
                else if (typeof(T) == typeof(bool))    value = element.GetBoolean();
                else
                    return MdixError.TypeMismatch(path, typeof(T).Name, element.ValueKind.ToString());
                return MdixResult<T>.Ok((T)value!);
            }
            catch (Exception ex)
            {
                return MdixError.ParseError($"Failed to deserialize tuple element at '{path}': {ex.Message}");
            }
        }

        private static MdixResult<JsonElement[]> ParseJsonArray(string json, string path, int expectedLength)
        {
            try
            {
                var doc  = JsonDocument.Parse(json);
                var root = doc.RootElement;

                if (root.ValueKind != JsonValueKind.Array)
                    return MdixError.TypeMismatch(path, $"tuple[{expectedLength}]", root.ValueKind.ToString());

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
}
