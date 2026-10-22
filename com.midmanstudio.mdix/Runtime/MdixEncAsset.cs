using System;
using System.Threading;
using System.Threading.Tasks;
using UnityEngine;
using MidManStudio.Mdix.Core;

namespace MidManStudio.Mdix.Unity
{
    /// <summary>
    /// Unity representation of a .mdix.enc file: the encrypted bytes, nothing else.
    /// Produced by MdixEncImporter when Unity imports a .mdix.enc asset.
    ///
    /// Drag it into an Inspector field and it is included in the build like any other asset,
    /// on every platform (no file path needed). It cannot be read without its key, and the key
    /// is NOT part of this asset: provide it from your own code with <see cref="MdixKeys"/>,
    /// or pass it to <see cref="LoadWithKey"/> / <see cref="LoadAsync"/>.
    ///
    /// The caller must dispose every database returned by a Load method.
    /// </summary>
    public sealed class MdixEncAsset : ScriptableObject
    {
        [SerializeField, HideInInspector]
        private byte[] _bytes = new byte[0];

        [SerializeField, HideInInspector]
        private string _keyName = string.Empty;

        [SerializeField, HideInInspector]
        private string _projectRelativePath = string.Empty;

        /// <summary>The name to register the key under: the file name without ".mdix.enc".</summary>
        public string KeyName => _keyName;

        /// <summary>Project path of the .mdix.enc file (editor use; just an identifier in a build).</summary>
        public string ProjectRelativePath => _projectRelativePath;

        /// <summary>Size of the encrypted data in bytes.</summary>
        public int SizeBytes => _bytes == null ? 0 : _bytes.Length;

        /// <summary>A copy of the encrypted bytes.</summary>
        public byte[] GetBytes()
        {
            if (_bytes == null) return new byte[0];
            var copy = new byte[_bytes.Length];
            Buffer.BlockCopy(_bytes, 0, copy, 0, _bytes.Length);
            return copy;
        }

        /// <summary>
        /// Decrypt with the key your code provided via <c>MdixKeys.Provide(KeyName, …)</c>.
        /// Fails with a clear message when no key was provided.
        /// </summary>
        public MdixResult<MdixDatabase> Load() => MdixKeys.LoadEncrypted(_keyName, _bytes);

        /// <summary>Decrypt with the text of a .mdix.key file you already hold.</summary>
        public MdixResult<MdixDatabase> LoadWithKey(string keyFileContent)
        {
            if (string.IsNullOrWhiteSpace(keyFileContent))
                return MdixError.NativeError("MdixEncAsset.LoadWithKey: key content is empty.");
            if (_bytes == null || _bytes.Length == 0)
                return MdixError.IoError(
                    "MdixEncAsset has no data — try reimporting the .mdix.enc file.");

            return MdixDatabase.LoadEncryptedBytes(_bytes, keyFileContent, null);
        }

        /// <summary>
        /// Decrypt a password-mode file: <paramref name="keyFileContent"/> is the key file (it holds
        /// the salt and parameters, not the key) and <paramref name="password"/> is the secret.
        /// </summary>
        public MdixResult<MdixDatabase> LoadWithPassword(string keyFileContent, string password)
        {
            if (string.IsNullOrEmpty(password))
                return MdixError.NativeError("MdixEncAsset.LoadWithPassword: password is empty.");
            if (string.IsNullOrWhiteSpace(keyFileContent))
                return MdixError.NativeError("MdixEncAsset.LoadWithPassword: key file content is empty.");
            if (_bytes == null || _bytes.Length == 0)
                return MdixError.IoError(
                    "MdixEncAsset has no data — try reimporting the .mdix.enc file.");

            return MdixDatabase.LoadEncryptedBytes(_bytes, keyFileContent, password);
        }

        /// <summary>
        /// Decrypt with a key from a custom provider (your server, a vault, a secure enclave…).
        /// The provider is handed <see cref="KeyName"/> as the identifier of the key it should return.
        /// </summary>
        public async Task<MdixResult<MdixDatabase>> LoadAsync(
            IMdixKeyProvider  provider,
            CancellationToken ct = default)
        {
            if (provider == null)
                return MdixError.NativeError("MdixEncAsset.LoadAsync: provider cannot be null.");

            var keyResult = await provider
                .GetKeyContentAsync(_keyName, ct)
                .ConfigureAwait(false);

            if (keyResult.IsFailure)
                return MdixResult<MdixDatabase>.Err(keyResult.Error);

            return LoadWithKey(keyResult.SuccessResult);
        }

        /// <summary>
        /// Decrypt with the provided key and read the data straight into a class or struct of type T
        /// (same binding as <see cref="MdixAsset.LoadAs{T}"/>). The database is disposed before this returns.
        /// </summary>
        public MdixResult<T> LoadAs<T>(string prefix = null)
        {
            var loadResult = Load();
            if (loadResult.IsFailure)
                return MdixResult<T>.Err(loadResult.Error);

            using var db = loadResult.SuccessResult;
            return MdixBinder.ReadAs<T>(db, MdixBinder.EffectivePrefix(typeof(T), prefix));
        }

        // Called by MdixEncImporter — not public API.
        public void SetData(byte[] bytes, string keyName, string projectRelativePath)
        {
            _bytes               = bytes               ?? new byte[0];
            _keyName             = keyName             ?? string.Empty;
            _projectRelativePath = projectRelativePath ?? string.Empty;
        }
    }
}
