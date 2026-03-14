using System;
using System.Runtime.InteropServices;
using MidManStudio.DixScript.Native;

namespace MidManStudio.Mdix.Core
{
    /// <summary>Controls how DixScript source or database content is formatted.</summary>
    public enum MdixFormatMode
    {
        /// <summary>Readable output with standard 2-space indentation.</summary>
        Default  = 0,
        /// <summary>Readable output with 4-space indentation and sorted keys.</summary>
        Pretty   = 1,
        /// <summary>Compact output — trailing whitespace removed, blank lines collapsed.</summary>
        Compact  = 2,
        /// <summary>Smallest possible output — all unnecessary whitespace removed.</summary>
        Minified = 3,
    }

    /// <summary>
    /// Format conversion utilities backed by the native Rust DixConverter and DixCompactor.
    /// All computation happens in the Rust layer — every method here is a thin FFI call-through.
    /// </summary>
    public static unsafe class MdixConverter
    {
        // ── .mdix export ──────────────────────────────────────────────────────

        /// <summary>
        /// Re-serializes a loaded database back to .mdix text using the Rust DixConverter.
        /// The output is a flat @DATA section — original table/group structure is not preserved
        /// because the runtime stores only the flattened hashmap.
        /// </summary>
        public static MdixResult<string> ToMdix(MdixDatabase db, MdixFormatMode mode = MdixFormatMode.Default)
        {
            if (db is null) return MdixError.NativeError("ToMdix: db cannot be null.");
            MdixNative.mdix_clear_error();

            if (!db.TryGetRawHandleInternal(out var handle))
                return MdixError.Disposed(nameof(MdixDatabase));

            try
            {
                // mdix_to_mdix returns void* in the generated binding — cast to byte*.
                var ptr = (byte*)MdixNative.mdix_to_mdix(handle, (MdixFormatMode)(int)mode);
                if (ptr == null)
                    return MdixError.NativeError(ReadLastError() ?? "mdix_to_mdix returned null.");
                return MdixResult<string>.Ok(ReadFreeString(ptr)!);
            }
            finally { db.ReleaseRawHandleInternal(); }
        }

        // ── JSON export / import ──────────────────────────────────────────────

        /// <summary>
        /// Exports all entries in the database as a JSON string via the Rust DixConverter.
        /// Dotted-path nesting is reconstructed automatically.
        /// </summary>
        public static MdixResult<string> ToJson(MdixDatabase db, bool indented = true)
        {
            if (db is null) return MdixError.NativeError("ToJson: db cannot be null.");
            MdixNative.mdix_clear_error();

            if (!db.TryGetRawHandleInternal(out var handle))
                return MdixError.Disposed(nameof(MdixDatabase));

            try
            {
                var ptr = MdixNative.mdix_to_json(handle, indented);
                if (ptr == null)
                    return MdixError.NativeError(ReadLastError() ?? "mdix_to_json returned null.");
                return MdixResult<string>.Ok(ReadFreeString(ptr)!);
            }
            finally { db.ReleaseRawHandleInternal(); }
        }

        /// <summary>
        /// Parses a JSON object string and returns a loaded database handle.
        /// The JSON must be an object at the top level — arrays are rejected.
        /// The returned database must be disposed when done.
        /// </summary>
        public static MdixResult<MdixDatabase> FromJson(string json)
        {
            if (string.IsNullOrEmpty(json))
                return MdixError.NativeError("FromJson: json cannot be null or empty.");

            MdixNative.mdix_clear_error();

            fixed (byte* srcPtr = MdixStringCache.GetUtf8Bytes(json))
            {
                var handle = MdixNative.mdix_from_json(srcPtr);
                if (handle == null)
                    return MdixError.NativeError(ReadLastError() ?? "mdix_from_json returned null.");

                return MdixResult<MdixDatabase>.Ok(MdixDatabase.FromRawHandle(handle));
            }
        }

        // ── TOML export / import ──────────────────────────────────────────────

        /// <summary>
        /// Exports all entries in the database as a TOML string via the Rust DixConverter.
        /// </summary>
        public static MdixResult<string> ToToml(MdixDatabase db)
        {
            if (db is null) return MdixError.NativeError("ToToml: db cannot be null.");
            MdixNative.mdix_clear_error();

            if (!db.TryGetRawHandleInternal(out var handle))
                return MdixError.Disposed(nameof(MdixDatabase));

            try
            {
                var ptr = MdixNative.mdix_to_toml(handle);
                if (ptr == null)
                    return MdixError.NativeError(ReadLastError() ?? "mdix_to_toml returned null.");
                return MdixResult<string>.Ok(ReadFreeString(ptr)!);
            }
            finally { db.ReleaseRawHandleInternal(); }
        }

        /// <summary>
        /// Parses a TOML table string and returns a loaded database handle.
        /// The TOML must be a table at the top level.
        /// The returned database must be disposed when done.
        /// </summary>
        public static MdixResult<MdixDatabase> FromToml(string toml)
        {
            if (string.IsNullOrEmpty(toml))
                return MdixError.NativeError("FromToml: toml cannot be null or empty.");

            MdixNative.mdix_clear_error();

            fixed (byte* srcPtr = MdixStringCache.GetUtf8Bytes(toml))
            {
                var handle = MdixNative.mdix_from_toml(srcPtr);
                if (handle == null)
                    return MdixError.NativeError(ReadLastError() ?? "mdix_from_toml returned null.");

                return MdixResult<MdixDatabase>.Ok(MdixDatabase.FromRawHandle(handle));
            }
        }

        // ── Source text formatting ─────────────────────────────────────────────

        /// <summary>
        /// Formats a raw .mdix source string using the Rust DixCompactor.
        /// Operates at the text level — no full grammar parse is performed.
        /// </summary>
        public static MdixResult<string> FormatSource(string source, MdixFormatMode mode = MdixFormatMode.Default)
        {
            if (source is null) return MdixError.NativeError("FormatSource: source cannot be null.");
            MdixNative.mdix_clear_error();

            fixed (byte* srcPtr = MdixStringCache.GetUtf8Bytes(source))
            {
                var ptr = MdixNative.mdix_format_source(srcPtr, (MdixFormatMode)(int)mode);
                if (ptr == null)
                    return MdixError.NativeError(ReadLastError() ?? "mdix_format_source returned null.");
                return MdixResult<string>.Ok(ReadFreeString(ptr)!);
            }
        }

        /// <summary>
        /// Removes all unnecessary whitespace and comments from a raw .mdix source string.
        /// String literal contents are preserved. Delegates to Rust DixCompactor::minify.
        /// </summary>
        public static MdixResult<string> MinifySource(string source)
        {
            if (source is null) return MdixError.NativeError("MinifySource: source cannot be null.");
            MdixNative.mdix_clear_error();

            fixed (byte* srcPtr = MdixStringCache.GetUtf8Bytes(source))
            {
                var ptr = MdixNative.mdix_minify_source(srcPtr);
                if (ptr == null)
                    return MdixError.NativeError(ReadLastError() ?? "mdix_minify_source returned null.");
                return MdixResult<string>.Ok(ReadFreeString(ptr)!);
            }
        }

        // ── Private helpers ───────────────────────────────────────────────────

        private static string? ReadLastError()
        {
            var ptr = MdixNative.mdix_get_last_error();
            return ptr == null ? null : Marshal.PtrToStringUTF8((IntPtr)ptr);
        }

        private static string? ReadFreeString(byte* ptr)
        {
            if (ptr == null) return null;
            try   { return Marshal.PtrToStringUTF8((IntPtr)ptr); }
            finally { MdixNative.mdix_free_string(ptr); }
        }
    }
}
