using System;
using System.Collections.Concurrent;
using System.Text;

namespace MidManStudio.Mdix.Core.Internal
{
    /// <summary>
    /// Thread-safe UTF-8 null-terminated byte cache for FFI path strings.
    /// Each unique string is encoded exactly once. Every subsequent call for
    /// the same string returns the cached array — zero allocation on the hot path.
    /// Pin the returned array with <c>fixed (byte* p = array)</c> before passing
    /// to any native function.
    /// </summary>
    internal static class MdixStringCache
    {
        #region Fields

        private static readonly ConcurrentDictionary<string, byte[]> Cache =
            new ConcurrentDictionary<string, byte[]>(StringComparer.Ordinal);

        #endregion

        #region Public API

        /// <summary>
        /// Returns a null-terminated UTF-8 byte array for <paramref name="value"/>.
        /// The array is cached — the same instance is returned on repeated calls.
        /// </summary>
        /// <exception cref="ArgumentNullException">
        /// Thrown if <paramref name="value"/> is null.
        /// </exception>
        public static byte[] GetUtf8Bytes(string value)
        {
            if (value is null) throw new ArgumentNullException(nameof(value));
            return Cache.GetOrAdd(value, EncodeNullTerminated);
        }

        /// <summary>
        /// Encodes <paramref name="value"/> to a fresh null-terminated UTF-8 byte
        /// array without caching. Use this for sensitive strings such as passwords
        /// that should not persist in memory beyond the call site.
        /// </summary>
        public static byte[] EncodeTemporary(string value)
        {
            if (value is null) throw new ArgumentNullException(nameof(value));
            return EncodeNullTerminated(value);
        }

        /// <summary>Removes all cached entries. Useful in low-memory situations or tests.</summary>
        public static void Clear() => Cache.Clear();

        /// <summary>The number of strings currently held in the cache.</summary>
        public static int Count => Cache.Count;

        #endregion

        #region Private Helpers

        private static byte[] EncodeNullTerminated(string s)
        {
            var utf8Bytes = Encoding.UTF8.GetBytes(s);
            var result    = new byte[utf8Bytes.Length + 1]; // +1 for null terminator
            Buffer.BlockCopy(utf8Bytes, 0, result, 0, utf8Bytes.Length);
            result[utf8Bytes.Length] = 0;
            return result;
        }

        #endregion
    }
}
