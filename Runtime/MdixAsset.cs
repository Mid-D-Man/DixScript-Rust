using System;
using UnityEngine;
using MidManStudio.Mdix.Core;

namespace MidManStudio.Mdix.Unity
{
    /// <summary>
    /// Unity representation of a .mdix file.
    /// Produced by MdixImporter when Unity imports a .mdix asset.
    ///
    /// Drag into Inspector fields to reference a mdix data file.
    /// Call Load() at runtime to get a MdixDatabase for querying.
    /// The caller is responsible for disposing the returned database.
    ///
    /// For encrypted files, use the Dix.Load* variants directly with
    /// your key/password strategy — see MdixKeyStorage for helpers.
    /// </summary>
    public sealed class MdixAsset : ScriptableObject
    {
        [SerializeField, HideInInspector]
        private string _rawSource = string.Empty;

        [SerializeField, HideInInspector]
        private string _projectRelativePath = string.Empty;

        /// <summary>Raw .mdix source text as it exists in the file.</summary>
        public string RawSource => _rawSource;

        /// <summary>
        /// Path to the .mdix file relative to the project root (Assets/...).
        /// Use this with Application.dataPath to build a full runtime path if needed.
        /// </summary>
        public string ProjectRelativePath => _projectRelativePath;

        /// <summary>
        /// Parse the source text and return a MdixDatabase ready for querying.
        /// The caller must dispose the returned database when done.
        ///
        /// Returns a failed MdixResult if the source is empty or invalid.
        /// </summary>
        public MdixResult<MdixDatabase> Load()
        {
            if (string.IsNullOrEmpty(_rawSource))
                return MdixError.NativeError(
                    "MdixAsset.Load: asset has no source data — try reimporting the .mdix file.");

            return Dix.LoadStr(_rawSource);
        }

        /// <summary>
        /// Deserialize the root DATA section directly into a POCO of type T.
        /// Combines Load() + db.Deserialize<T>() in one call.
        /// No database handle to manage — deserialization happens and the
        /// database is disposed before this returns.
        /// </summary>
        public MdixResult<T> LoadAs<T>(string? prefix = null)
        {
            var loadResult = Load();
            if (loadResult.IsFailure)
                return MdixResult<T>.Err(loadResult.Error);

            using var db = loadResult.SuccessResult;
            return db.Deserialize<T>(prefix);
        }

        // Called by MdixImporter — not public API.
        internal void SetData(string rawSource, string projectRelativePath)
        {
            _rawSource            = rawSource            ?? string.Empty;
            _projectRelativePath  = projectRelativePath  ?? string.Empty;
        }
    }
}
