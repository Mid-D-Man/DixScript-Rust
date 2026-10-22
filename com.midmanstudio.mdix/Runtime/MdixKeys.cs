using System;
using System.Collections.Generic;
using MidManStudio.Mdix.Core;

namespace MidManStudio.Mdix.Unity
{
    /// <summary>
    /// Where your own code hands an encryption key to MDIX.
    ///
    /// An encrypted .mdix.enc ships inside the build, but its .mdix.key never does. At runtime
    /// the only way the data opens is code YOU wrote calling <see cref="Provide(string, string)"/>
    /// (or one of the overloads) with the key. Nothing in MDIX reads a key from disk on its own
    /// on this path, so a player who unpacks the game finds ciphertext and no key file.
    ///
    ///   // once, early — the key comes from wherever YOU decide: your server, a login
    ///   // response, a value derived at runtime…
    ///   MdixKeys.Provide("secrets", () => myServerKey);
    ///
    ///   // anywhere after that
    ///   using var db = secretsAsset.Load().Unwrap();
    ///
    /// The key name is the file name without ".mdix.enc": secrets.mdix.enc → "secrets".
    ///
    /// This is a process-wide registry held in memory only. It is not saved, not logged and not
    /// sent anywhere. Call <see cref="Revoke"/> once you have loaded what you need if you do not
    /// want the key to stay in memory.
    /// </summary>
    public static class MdixKeys
    {
        private static readonly object Gate = new object();

        private static readonly Dictionary<string, Func<string>> Sources =
            new Dictionary<string, Func<string>>(StringComparer.Ordinal);

        // ── Provide ───────────────────────────────────────────────────────────

        /// <summary>Register the full text of a .mdix.key file for <paramref name="keyName"/>.</summary>
        public static void Provide(string keyName, string keyFileContent)
        {
            if (string.IsNullOrWhiteSpace(keyFileContent))
                throw new ArgumentException("Key content is empty.", nameof(keyFileContent));

            Provide(keyName, () => keyFileContent);
        }

        /// <summary>
        /// Register a function that returns the text of a .mdix.key file. It is called each time
        /// the key is needed, so the key does not have to sit in a field for the life of the app:
        /// decode it, fetch it or derive it inside the function.
        /// </summary>
        public static void Provide(string keyName, Func<string> keyFileContent)
        {
            if (string.IsNullOrWhiteSpace(keyName))
                throw new ArgumentException("Key name is empty.", nameof(keyName));
            if (keyFileContent == null)
                throw new ArgumentNullException(nameof(keyFileContent));

            lock (Gate) { Sources[keyName] = keyFileContent; }
        }

        // ── Query / remove ────────────────────────────────────────────────────

        /// <summary>True when a key has been provided for this name.</summary>
        public static bool IsProvided(string keyName)
        {
            if (string.IsNullOrEmpty(keyName)) return false;
            lock (Gate) { return Sources.ContainsKey(keyName); }
        }

        /// <summary>Forget the key for this name.</summary>
        public static void Revoke(string keyName)
        {
            if (string.IsNullOrEmpty(keyName)) return;
            lock (Gate) { Sources.Remove(keyName); }
        }

        /// <summary>Forget every provided key.</summary>
        public static void RevokeAll()
        {
            lock (Gate) { Sources.Clear(); }
        }

        // ── Use ───────────────────────────────────────────────────────────────

        /// <summary>Get the key file text provided for this name. Fails if none was provided.</summary>
        public static MdixResult<string> Resolve(string keyName)
        {
            Func<string> source = null;
            if (!string.IsNullOrEmpty(keyName))
            {
                lock (Gate) { Sources.TryGetValue(keyName, out source); }
            }

            if (source == null)
                return MdixError.NativeError(
                    $"No key was provided for '{keyName}'. Call MdixKeys.Provide(\"{keyName}\", …) " +
                    "from your own code before loading it (the .mdix.key is never shipped).");

            string content;
            try   { content = source(); }
            catch (Exception ex)
            {
                return MdixError.NativeError(
                    $"The key function for '{keyName}' threw: {ex.Message}");
            }

            if (string.IsNullOrWhiteSpace(content))
                return MdixError.NativeError(
                    $"The key function for '{keyName}' returned nothing.");

            return MdixResult<string>.Ok(content);
        }

        /// <summary>
        /// Decrypt <paramref name="encryptedBytes"/> with the key provided for
        /// <paramref name="keyName"/>. The caller must dispose the returned database.
        /// </summary>
        public static MdixResult<MdixDatabase> LoadEncrypted(string keyName, byte[] encryptedBytes)
        {
            if (encryptedBytes == null || encryptedBytes.Length == 0)
                return MdixError.IoError($"No encrypted data to load for '{keyName}'.");

            var key = Resolve(keyName);
            if (key.IsFailure)
                return MdixResult<MdixDatabase>.Err(key.Error);

            return MdixDatabase.LoadEncryptedBytes(encryptedBytes, key.SuccessResult, null);
        }
    }
}
