using System;
using System.IO;
using System.Threading;
using System.Threading.Tasks;
using UnityEngine;
using MidManStudio.Mdix.Core;

namespace MidManStudio.Mdix.Unity
{
    /// <summary>
    /// Platform-appropriate key file storage and retrieval helpers.
    ///
    /// Security tier summary:
    ///   Cloud retrieval   — highest, key never touches player's disk
    ///   Mobile sandbox    — medium, safe from other apps on unrooted devices
    ///   PC local          — low, player can always browse to persistentDataPath
    ///
    /// For sensitive data on PC, always use cloud key retrieval.
    /// For player save data on mobile, sandbox storage is reasonable.
    /// </summary>
    public static class MdixKeyStorage
    {
        // ── Path helpers ─────────────────────────────────────────────────────

        /// <summary>
        /// Returns the platform-appropriate directory for storing key files.
        /// On mobile this is the app's private sandbox directory.
        /// On PC this is persistentDataPath — not truly private, use cloud instead.
        /// </summary>
        public static string GetKeyStorageDirectory()
        {
            return Path.Combine(Application.persistentDataPath, ".mdix_keys");
        }

        /// <summary>
        /// Returns the full path for a key file stored in the platform directory.
        /// The key file name is derived from the encrypted file name automatically.
        /// </summary>
        public static string GetLocalKeyPath(string encFileName)
        {
            if (string.IsNullOrEmpty(encFileName))
                throw new ArgumentNullException(nameof(encFileName));

            var baseName = Path.GetFileName(
                MdixKeyUtilities.GetDefaultKeyPath(encFileName));

            return Path.Combine(GetKeyStorageDirectory(), baseName);
        }

        // ── Save key to local storage ─────────────────────────────────────────

        /// <summary>
        /// Writes key file content to platform-local storage.
        /// On mobile this is inside the app sandbox — other apps cannot read it.
        /// On PC this provides no real security — use cloud keys instead.
        /// </summary>
        public static MdixResult<Unit> SaveKeyLocally(string keyContent, string encFileName)
        {
            if (string.IsNullOrEmpty(keyContent))
                return MdixError.NativeError("SaveKeyLocally: keyContent cannot be empty.");
            if (string.IsNullOrEmpty(encFileName))
                return MdixError.InvalidPath(encFileName);

            try
            {
                var dir = GetKeyStorageDirectory();
                Directory.CreateDirectory(dir);

                var keyPath = GetLocalKeyPath(encFileName);
                File.WriteAllText(keyPath, keyContent);
                return MdixResult<Unit>.Ok(Unit.Value);
            }
            catch (Exception ex)
            {
                return MdixError.IoError(
                    $"SaveKeyLocally: failed to write key file: {ex.Message}", ex);
            }
        }

        // ── Load using local key ──────────────────────────────────────────────

        /// <summary>
        /// Load an encrypted .mdix.enc file using a key stored in platform-local storage.
        /// Returns a failed result if the key file is not found locally.
        /// </summary>
        public static MdixResult<MdixDatabase> LoadWithLocalKey(string encPath)
        {
            if (string.IsNullOrEmpty(encPath))
                return MdixError.InvalidPath(encPath);

            var localKeyPath = GetLocalKeyPath(encPath);
            if (!File.Exists(localKeyPath))
                return MdixError.IoError(
                    $"LoadWithLocalKey: key file not found at '{localKeyPath}'. " +
                    "Retrieve the key from your server and call SaveKeyLocally() first.");

            return MdixDatabase.LoadEncrypted(encPath, localKeyPath);
        }

        // ── Load using cloud key ──────────────────────────────────────────────

        /// <summary>
        /// Fetch the key from a cloud URL and load an encrypted .mdix.enc file.
        /// The key is never written to disk — it lives in memory only for the
        /// duration of this call.
        ///
        /// The URL must use HTTPS. Your server should require authentication
        /// before returning the key.
        /// </summary>
        public static async Task<MdixResult<MdixDatabase>> LoadWithCloudKeyAsync(
            string            encPath,
            string            keyUrl,
            CancellationToken ct = default)
        {
            return await MdixKeyUtilities.LoadEncryptedWithCloudKeyAsync(
                encPath, keyUrl, null, ct).ConfigureAwait(false);
        }

        /// <summary>
        /// Fetch the key from a cloud URL, load the encrypted file, then
        /// cache the key in local platform storage for offline use.
        ///
        /// Use this pattern for mobile games that need offline access after
        /// the first authenticated session.
        /// On PC this provides limited security — the cached key is readable.
        /// </summary>
        public static async Task<MdixResult<MdixDatabase>> LoadWithCloudKeyAndCacheAsync(
            string            encPath,
            string            keyUrl,
            CancellationToken ct = default)
        {
            var fetchResult = await MdixKeyUtilities.FetchKeyFromUrlAsync(keyUrl, ct)
                .ConfigureAwait(false);

            if (fetchResult.IsFailure)
                return MdixResult<MdixDatabase>.Err(fetchResult.Error);

            var keyContent = fetchResult.SuccessResult;

            // Cache locally for offline access — best-effort, non-fatal if it fails.
            SaveKeyLocally(keyContent, encPath);

            if (!File.Exists(encPath))
                return MdixError.IoError($"LoadWithCloudKeyAndCacheAsync: enc file not found at '{encPath}'.");

            byte[] encBytes;
            try   { encBytes = File.ReadAllBytes(encPath); }
            catch (Exception ex)
            {
                return MdixError.IoError(
                    $"LoadWithCloudKeyAndCacheAsync: cannot read enc file: {ex.Message}", ex);
            }

            return MdixDatabase.LoadEncryptedBytes(encBytes, keyContent, null);
        }

        // ── Custom provider ───────────────────────────────────────────────────

        /// <summary>
        /// Load an encrypted file using a custom key provider.
        /// Implement IMdixKeyProvider to plug in any key retrieval strategy —
        /// HSM, secure enclave, derived key, etc.
        /// </summary>
        public static async Task<MdixResult<MdixDatabase>> LoadWithProviderAsync(
            string            encPath,
            IMdixKeyProvider  provider,
            CancellationToken ct = default)
        {
            if (provider is null)
                return MdixError.NativeError("LoadWithProviderAsync: provider cannot be null.");
            if (string.IsNullOrEmpty(encPath))
                return MdixError.InvalidPath(encPath);

            var keyResult = await provider.GetKeyContentAsync(encPath, ct)
                .ConfigureAwait(false);

            if (keyResult.IsFailure)
                return MdixResult<MdixDatabase>.Err(keyResult.Error);

            if (!File.Exists(encPath))
                return MdixError.IoError($"Enc file not found: '{encPath}'.");

            byte[] encBytes;
            try   { encBytes = File.ReadAllBytes(encPath); }
            catch (Exception ex)
            {
                return MdixError.IoError(
                    $"LoadWithProviderAsync: cannot read enc file: {ex.Message}", ex);
            }

            return MdixDatabase.LoadEncryptedBytes(encBytes, keyResult.SuccessResult, null);
        }
    }

    // ── IMdixKeyProvider ──────────────────────────────────────────────────────

    /// <summary>
    /// Implement this interface to provide a custom key retrieval strategy.
    /// Passed to MdixKeyStorage.LoadWithProviderAsync().
    /// </summary>
    public interface IMdixKeyProvider
    {
        /// <summary>
        /// Return the full text content of the .mdix.key file for the given
        /// encrypted file path. Called once per load — do not cache the result
        /// here; MdixKeyStorage handles caching if needed.
        /// </summary>
        Task<MdixResult<string>> GetKeyContentAsync(
            string            encPath,
            CancellationToken ct);
    }
}
