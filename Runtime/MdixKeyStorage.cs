using System;
using System.IO;
using System.Threading;
using System.Threading.Tasks;
using UnityEngine;
using MidManStudio.Mdix.Core;

namespace MidManStudio.Mdix.Unity
{
    /// <summary>
    /// Platform-appropriate key file storage and retrieval.
    /// All key files are stored under MdixPaths.Keys —
    /// persistentDataPath/mdix/.keys/
    ///
    /// Security summary:
    ///   Cloud retrieval  — key never touches player disk, highest security
    ///   Mobile sandbox   — safe from other apps on unrooted devices
    ///   PC local         — not truly private, use cloud for sensitive data on PC
    /// </summary>
    public static class MdixKeyStorage
    {
        // ── Save key locally ──────────────────────────────────────────────────

        /// <summary>
        /// Write key file content into the platform key storage directory.
        /// On mobile this is inside the app sandbox.
        /// On PC this is persistentDataPath/mdix/.keys/ — not truly private.
        /// </summary>
        public static MdixResult<Unit> SaveKeyLocally(
            string keyContent,
            string encFileName)
        {
            if (string.IsNullOrEmpty(keyContent))
                return MdixError.NativeError(
                    "SaveKeyLocally: keyContent cannot be empty.");
            if (string.IsNullOrEmpty(encFileName))
                return MdixError.InvalidPath(encFileName);

            try
            {
                MdixPaths.EnsureDirectoriesExist();

                File.WriteAllText(
                    MdixPaths.KeyFile(encFileName),
                    keyContent);

                return MdixResult<Unit>.Ok(Unit.Value);
            }
            catch (Exception ex)
            {
                return MdixError.IoError(
                    $"SaveKeyLocally: failed to write key: {ex.Message}", ex);
            }
        }

        // ── Load using local key ──────────────────────────────────────────────

        /// <summary>
        /// Load an encrypted .mdix.enc file using a key stored locally
        /// under MdixPaths.Keys.
        /// Returns a failed result if the key file is not found.
        /// </summary>
        public static MdixResult<MdixDatabase> LoadWithLocalKey(string encPath)
        {
            if (string.IsNullOrEmpty(encPath))
                return MdixError.InvalidPath(encPath);

            var keyPath = MdixPaths.KeyFile(encPath);

            if (!File.Exists(keyPath))
                return MdixError.IoError(
                    $"LoadWithLocalKey: key not found at '{keyPath}'. " +
                    "Retrieve the key from your server and call " +
                    "SaveKeyLocally() first.");

            return MdixDatabase.LoadEncrypted(encPath, keyPath);
        }

        // ── Load using cloud key ──────────────────────────────────────────────

        /// <summary>
        /// Fetch the key from an HTTPS URL and load an encrypted file.
        /// The key is never written to disk — memory only for this call.
        /// Your server should require authentication before returning the key.
        /// </summary>
        public static async Task<MdixResult<MdixDatabase>> LoadWithCloudKeyAsync(
            string            encPath,
            string            keyUrl,
            CancellationToken ct = default)
        {
            return await MdixKeyUtilities
                .LoadEncryptedWithCloudKeyAsync(encPath, keyUrl, null, ct)
                .ConfigureAwait(false);
        }

        /// <summary>
        /// Fetch key from cloud, load the encrypted file, then cache
        /// the key locally under MdixPaths.Keys for offline use.
        ///
        /// Good pattern for mobile: authenticate once, cache key, use offline.
        /// On PC the cached key is readable — use direct cloud fetch instead.
        /// </summary>
        public static async Task<MdixResult<MdixDatabase>>
            LoadWithCloudKeyAndCacheAsync(
                string            encPath,
                string            keyUrl,
                CancellationToken ct = default)
        {
            var fetchResult = await MdixKeyUtilities
                .FetchKeyFromUrlAsync(keyUrl, ct)
                .ConfigureAwait(false);

            if (fetchResult.IsFailure)
                return MdixResult<MdixDatabase>.Err(fetchResult.Error);

            var keyContent = fetchResult.SuccessResult;

            // Best-effort cache — non-fatal if it fails.
            SaveKeyLocally(keyContent, encPath);

            if (!File.Exists(encPath))
                return MdixError.IoError(
                    $"LoadWithCloudKeyAndCacheAsync: enc file not found: '{encPath}'.");

            byte[] encBytes;
            try   { encBytes = File.ReadAllBytes(encPath); }
            catch (Exception ex)
            {
                return MdixError.IoError(
                    $"LoadWithCloudKeyAndCacheAsync: cannot read enc file: " +
                    $"{ex.Message}", ex);
            }

            return MdixDatabase.LoadEncryptedBytes(encBytes, keyContent, null);
        }

        // ── Custom provider ───────────────────────────────────────────────────

        /// <summary>
        /// Load an encrypted file using a custom key provider.
        /// Implement IMdixKeyProvider for HSM, secure enclave,
        /// derived-key, or any other strategy.
        /// </summary>
        public static async Task<MdixResult<MdixDatabase>> LoadWithProviderAsync(
            string            encPath,
            IMdixKeyProvider  provider,
            CancellationToken ct = default)
        {
            if (provider is null)
                return MdixError.NativeError(
                    "LoadWithProviderAsync: provider cannot be null.");
            if (string.IsNullOrEmpty(encPath))
                return MdixError.InvalidPath(encPath);

            var keyResult = await provider
                .GetKeyContentAsync(encPath, ct)
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
                    $"LoadWithProviderAsync: cannot read enc file: " +
                    $"{ex.Message}", ex);
            }

            return MdixDatabase.LoadEncryptedBytes(
                encBytes, keyResult.SuccessResult, null);
        }
    }

    // ── IMdixKeyProvider ──────────────────────────────────────────────────────

    /// <summary>
    /// Implement this to plug in a custom key retrieval strategy.
    /// </summary>
    public interface IMdixKeyProvider
    {
        Task<MdixResult<string>> GetKeyContentAsync(
            string            encPath,
            CancellationToken ct);
    }
}
