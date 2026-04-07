using System;
using System.IO;
using System.Net.Http;
using System.Threading;
using System.Threading.Tasks;

namespace MidManStudio.Mdix.Core
{
    // ══════════════════════════════════════════════════════════════════════════
    // MdixLoadOptions
    // ══════════════════════════════════════════════════════════════════════════

    /// <summary>
    /// Configures how an encrypted .mdix.enc file is loaded.
    /// Construct via the static factory methods rather than setting properties directly.
    /// </summary>
    public sealed class MdixLoadOptions
    {
        /// <summary>Password for decryption when the key was compiled with password mode.</summary>
        public string? Password { get; private set; }

        /// <summary>Explicit path to a .mdix.key file. Null means auto-detect next to the .enc file.</summary>
        public string? KeyFilePath { get; private set; }

        /// <summary>Full text content of a .mdix.key file loaded from a secure vault or memory.</summary>
        public string? KeyFileContent { get; private set; }

        /// <summary>HTTPS URL to fetch the .mdix.key file from. Requires <see cref="AllowUrlKeyLoading"/>.</summary>
        public string? KeyFileUrl { get; private set; }

        /// <summary>When true, URL-based key fetching is permitted. Defaults to false.</summary>
        public bool AllowUrlKeyLoading { get; private set; }

        /// <summary>When true, key content passed directly as a string is accepted. Defaults to false.</summary>
        public bool AllowDirectKeyContent { get; private set; }

        /// <summary>Additional directories to search for the .mdix.key file if not found next to the .enc file.</summary>
        public string[]? KeyFileSearchPaths { get; private set; }

        // ── Factories ─────────────────────────────────────────────────────────

        /// <summary>Default options — auto-detect key file, no password.</summary>
        public static MdixLoadOptions Default() => new MdixLoadOptions();

        /// <summary>Load using a password. The key file is auto-detected next to the .enc file.</summary>
        public static MdixLoadOptions WithPassword(string password)
        {
            if (string.IsNullOrEmpty(password))
                throw new ArgumentException("Password cannot be null or empty.", nameof(password));
            return new MdixLoadOptions { Password = password };
        }

        /// <summary>Load using an explicit key file path.</summary>
        public static MdixLoadOptions WithKeyFile(string keyFilePath)
        {
            if (string.IsNullOrEmpty(keyFilePath))
                throw new ArgumentException("Key file path cannot be null or empty.", nameof(keyFilePath));
            return new MdixLoadOptions { KeyFilePath = keyFilePath };
        }

        /// <summary>
        /// Load using key file content supplied directly as a string (e.g. from HashiCorp Vault or AWS Secrets Manager).
        /// <paramref name="acknowledgeSecurityRisk"/> must be <c>true</c> to confirm you understand that holding key
        /// material in a managed string exposes it to GC logs and memory dumps.
        /// </summary>
        public static MdixResult<MdixLoadOptions> WithKeyContent(
            string content,
            bool   acknowledgeSecurityRisk)
        {
            if (!acknowledgeSecurityRisk)
                return MdixError.NativeError(
                    "Direct key content loading requires explicit security acknowledgment. " +
                    "Set acknowledgeSecurityRisk = true only when loading from a trusted secure vault.");

            if (string.IsNullOrWhiteSpace(content))
                return MdixError.NativeError("Key file content cannot be null or empty.");

            if (content.Length < 50)
                return MdixError.NativeError(
                    "Key file content appears too short to be valid. " +
                    "Ensure you are providing the complete .mdix.key file content.");

            return MdixResult<MdixLoadOptions>.Ok(new MdixLoadOptions
            {
                KeyFileContent        = content,
                AllowDirectKeyContent = true,
            });
        }

        /// <summary>
        /// Load by fetching the key file from an HTTPS URL.
        /// <paramref name="acknowledgeSecurityRisk"/> must be <c>true</c> to confirm the URL is trusted.
        /// The URL must begin with <c>https://</c>.
        /// </summary>
        public static MdixResult<MdixLoadOptions> WithKeyUrl(
            string keyUrl,
            bool   acknowledgeSecurityRisk)
        {
            if (!acknowledgeSecurityRisk)
                return MdixError.NativeError(
                    "URL key loading requires explicit security acknowledgment. " +
                    "Set acknowledgeSecurityRisk = true only for HTTPS URLs from trusted internal services.");

            if (string.IsNullOrWhiteSpace(keyUrl))
                return MdixError.NativeError("Key URL cannot be null or empty.");

            if (!keyUrl.StartsWith("https://", StringComparison.OrdinalIgnoreCase))
                return MdixError.NativeError(
                    "Key URL must use the HTTPS protocol. HTTP is not permitted for key file loading.");

            return MdixResult<MdixLoadOptions>.Ok(new MdixLoadOptions
            {
                KeyFileUrl         = keyUrl,
                AllowUrlKeyLoading = true,
            });
        }

        /// <summary>
        /// Provide additional directories to search for the .mdix.key file when it cannot be found
        /// in the same directory as the .enc file.
        /// </summary>
        public static MdixLoadOptions WithKeySearchPaths(params string[] searchPaths)
        {
            if (searchPaths == null || searchPaths.Length == 0)
                throw new ArgumentException("At least one search path is required.", nameof(searchPaths));
            return new MdixLoadOptions { KeyFileSearchPaths = searchPaths };
        }

        // ── Validation ────────────────────────────────────────────────────────

        /// <summary>
        /// Validates that the options are internally consistent.
        /// Returns an error if multiple mutually-exclusive key sources are specified,
        /// or if security acknowledgments are missing.
        /// </summary>
        public MdixResult<Unit> Validate()
        {
            var keyOptionCount = 0;
            if (KeyFilePath    != null) keyOptionCount++;
            if (KeyFileContent != null) keyOptionCount++;
            if (KeyFileUrl     != null) keyOptionCount++;

            if (keyOptionCount > 1)
                return MdixError.NativeError(
                    "Cannot specify more than one key source. Use only one of: " +
                    "WithKeyFile, WithKeyContent, or WithKeyUrl.");

            if (KeyFileUrl != null)
            {
                if (!AllowUrlKeyLoading)
                    return MdixError.NativeError(
                        "URL key loading is disabled. Use MdixLoadOptions.WithKeyUrl() with acknowledgeSecurityRisk = true.");
                if (!KeyFileUrl.StartsWith("https://", StringComparison.OrdinalIgnoreCase))
                    return MdixError.NativeError("Key file URL must use HTTPS.");
            }

            if (KeyFileContent != null && !AllowDirectKeyContent)
                return MdixError.NativeError(
                    "Direct key content is disabled. Use MdixLoadOptions.WithKeyContent() with acknowledgeSecurityRisk = true.");

            return MdixResult<Unit>.Ok(Unit.Value);
        }

        /// <summary>
        /// Resolves this set of options and calls the appropriate <see cref="MdixDatabase"/> load overload.
        /// For URL-based keys, fetch the key content first with
        /// <see cref="MdixKeyUtilities.FetchKeyFromUrlAsync"/> then use <see cref="WithKeyContent"/>.
        /// </summary>
        public MdixResult<MdixDatabase> Apply(string encPath)
        {
            var validationResult = Validate();
            if (validationResult.IsFailure) return MdixResult<MdixDatabase>.Err(validationResult.Error);

            if (KeyFileUrl != null)
                return MdixError.NativeError(
                    "URL-based key loading is async. Use MdixKeyUtilities.FetchKeyFromUrlAsync() " +
                    "to retrieve the key content, then call MdixDatabase.LoadEncryptedBytes().");

            if (KeyFileContent != null)
            {
                var data = File.ReadAllBytes(encPath);
                return MdixDatabase.LoadEncryptedBytes(data, KeyFileContent, Password);
            }

            if (Password != null && KeyFilePath == null)
                return MdixDatabase.LoadEncryptedPassword(encPath, Password);

            if (KeyFilePath != null)
                return MdixDatabase.LoadEncrypted(encPath, KeyFilePath);

            // Auto-detect or use search paths
            var keyPath = MdixKeyUtilities.TryFindKeyFile(encPath, KeyFileSearchPaths);
            return keyPath != null
                ? MdixDatabase.LoadEncrypted(encPath, keyPath)
                : MdixDatabase.LoadEncrypted(encPath, null);
        }
    }

    // ══════════════════════════════════════════════════════════════════════════
    // MdixKeyUtilities
    // ══════════════════════════════════════════════════════════════════════════

    /// <summary>
    /// Pure C# helpers for locating, validating, and fetching .mdix.key files.
    /// No native FFI is involved — all operations are managed file I/O or HTTP.
    /// </summary>
    public static class MdixKeyUtilities
    {
        // ── Path helpers ──────────────────────────────────────────────────────

        /// <summary>
        /// Returns the conventional key file path for a given encrypted file.
        /// For <c>secrets.mdix.enc</c> → <c>secrets.mdix.key</c> in the same directory.
        /// </summary>
        public static string GetDefaultKeyPath(string encryptedFilePath)
        {
            if (string.IsNullOrEmpty(encryptedFilePath))
                throw new ArgumentException("Path cannot be null or empty.", nameof(encryptedFilePath));

            var dir      = Path.GetDirectoryName(encryptedFilePath) ?? string.Empty;
            var fileName = Path.GetFileName(encryptedFilePath);

            // Strip .enc suffix if present, then append .key
            var baseName = fileName.EndsWith(".enc", StringComparison.OrdinalIgnoreCase)
                ? fileName.Substring(0, fileName.Length - 4)
                : fileName;

            return Path.Combine(dir, baseName + ".key");
        }

        /// <summary>
        /// Searches for the key file next to the encrypted file, then in any additional
        /// <paramref name="searchPaths"/>. Returns the first path found, or null.
        /// </summary>
        public static string? TryFindKeyFile(string encryptedFilePath, string[]? searchPaths = null)
        {
            // 1. Default location next to the .enc file
            var defaultPath = GetDefaultKeyPath(encryptedFilePath);
            if (File.Exists(defaultPath)) return defaultPath;

            // 2. Extra search paths
            if (searchPaths != null)
            {
                var fileName = Path.GetFileName(defaultPath);
                foreach (var dir in searchPaths)
                {
                    var candidate = Path.Combine(dir, fileName);
                    if (File.Exists(candidate)) return candidate;
                }
            }

            return null;
        }

        // ── Validation ────────────────────────────────────────────────────────

        /// <summary>
        /// Reads a key file from disk and performs a basic structural validation.
        /// Checks that the file exists, is non-empty, and contains recognizable key markers.
        /// Does not decrypt or verify cryptographic integrity — that happens at load time.
        /// </summary>
        public static MdixResult<Unit> ValidateKeyFile(string keyFilePath)
        {
            if (string.IsNullOrEmpty(keyFilePath))
                return MdixError.InvalidPath(keyFilePath);

            if (!File.Exists(keyFilePath))
                return MdixError.IoError($"Key file not found: '{keyFilePath}'");

            string content;
            try   { content = File.ReadAllText(keyFilePath); }
            catch (Exception ex) { return MdixError.IoError($"Cannot read key file: {ex.Message}", ex); }

            return ValidateKeyFileContent(content);
        }

        /// <summary>
        /// Validates the content string of a .mdix.key file without touching the filesystem.
        /// </summary>
        public static MdixResult<Unit> ValidateKeyFileContent(string content)
        {
            if (string.IsNullOrWhiteSpace(content))
                return MdixError.NativeError("Key file content is empty.");

            if (content.Length < 50)
                return MdixError.NativeError(
                    "Key file content is too short to be a valid .mdix.key file.");

            // A valid key file contains at least one of these structural markers.
            var hasMarker =
                content.Contains("@CONFIG",      StringComparison.Ordinal) ||
                content.Contains("@KEY_DATA",    StringComparison.Ordinal) ||
                content.Contains("\"version\"",  StringComparison.Ordinal) ||
                content.Contains("\"algorithm\"", StringComparison.Ordinal);

            if (!hasMarker)
                return MdixError.NativeError(
                    "Key file does not appear to contain valid DixScript key data. " +
                    "Ensure you are providing the complete .mdix.key file.");

            return MdixResult<Unit>.Ok(Unit.Value);
        }

        // ── Cloud key fetching ────────────────────────────────────────────────

        /// <summary>
        /// Fetches key file content from an HTTPS URL.
        /// The URL must begin with <c>https://</c>. HTTP is rejected.
        /// Dispose the returned content string as soon as possible if security is critical.
        /// </summary>
        public static async Task<MdixResult<string>> FetchKeyFromUrlAsync(
            string            url,
            CancellationToken ct = default)
        {
            if (string.IsNullOrEmpty(url))
                return MdixError.NativeError("Key URL cannot be null or empty.");

            if (!url.StartsWith("https://", StringComparison.OrdinalIgnoreCase))
                return MdixError.NativeError("Key URL must use HTTPS. HTTP is not permitted.");

            try
            {
                using var client  = new HttpClient { Timeout = TimeSpan.FromSeconds(15) };
                var content = await client.GetStringAsync(url
#if NETCOREAPP3_0_OR_GREATER || NETSTANDARD2_1
                    // CancellationToken overload available on newer runtimes only
#endif
                ).ConfigureAwait(false);

                ct.ThrowIfCancellationRequested();

                var validationResult = ValidateKeyFileContent(content);
                return validationResult.IsFailure
                    ? MdixResult<string>.Err(validationResult.Error)
                    : MdixResult<string>.Ok(content);
            }
            catch (OperationCanceledException)
            {
                return MdixError.IoError("Key fetch cancelled.");
            }
            catch (HttpRequestException ex)
            {
                return MdixError.IoError($"Failed to fetch key from URL '{url}': {ex.Message}", ex);
            }
            catch (Exception ex)
            {
                return MdixError.IoError($"Unexpected error fetching key from URL: {ex.Message}", ex);
            }
        }

        /// <summary>
        /// Convenience method: fetches the key from <paramref name="keyUrl"/> and immediately loads
        /// the encrypted file. The key content is discarded after the load completes.
        /// </summary>
        public static async Task<MdixResult<MdixDatabase>> LoadEncryptedWithCloudKeyAsync(
            string            encPath,
            string            keyUrl,
            string?           password = null,
            CancellationToken ct       = default)
        {
            var fetchResult = await FetchKeyFromUrlAsync(keyUrl, ct).ConfigureAwait(false);
            if (fetchResult.IsFailure) return MdixResult<MdixDatabase>.Err(fetchResult.Error);

            var keyContent = fetchResult.SuccessResult;

            if (!File.Exists(encPath))
                return MdixError.IoError($"Encrypted file not found: '{encPath}'");

            byte[] encBytes;
            try   { encBytes = File.ReadAllBytes(encPath); }
            catch (Exception ex) { return MdixError.IoError($"Cannot read encrypted file: {ex.Message}", ex); }

            return MdixDatabase.LoadEncryptedBytes(encBytes, keyContent, password);
        }

        /// <summary>
        /// Async variant: reads the encrypted bytes from disk on a background thread,
        /// fetches the key from the URL, and loads the database.
        /// </summary>
        public static Task<MdixResult<MdixDatabase>> LoadEncryptedWithCloudKeyAsync(
            string            encPath,
            string            keyUrl,
            MdixLoadOptions   options,
            CancellationToken ct = default)
        {
            var pw = options.Password;
            return LoadEncryptedWithCloudKeyAsync(encPath, keyUrl, pw, ct);
        }
    }
}
