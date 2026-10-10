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
        ///
        /// The serializer maps public properties only and cannot fill List or
        /// array members. For classes with public fields or lists, use Bind&lt;T&gt;.
        /// </summary>
        public MdixResult<T> LoadAs<T>(string? prefix = null)
        {
            var loadResult = Load();
            if (loadResult.IsFailure)
                return MdixResult<T>.Err(loadResult.Error);

            using var db = loadResult.SuccessResult;
            return db.Deserialize<T>(prefix);
        }

        /// <summary>
        /// Build a new T and fill it from the data at <paramref name="dataPath"/> (empty = root).
        /// Unlike LoadAs, this reads public fields, [SerializeField] fields and writable
        /// properties, and fills lists, arrays, nested classes and enums to any depth.
        /// Keys are matched to members as SpawnCap, spawnCap or spawn_cap.
        /// Fails only when data exists but cannot be converted; see <see cref="MdixBinder"/>
        /// to get a report of missing members and unused keys.
        /// The database is disposed before this returns.
        /// T must be a plain class or struct, not a ScriptableObject.
        ///
        /// Works by reflection: on IL2CPP, keep T and its nested types from being
        /// stripped ([Preserve] or a link.xml entry for the assembly that holds them).
        /// </summary>
        public MdixResult<T> Bind<T>(string dataPath = "") where T : new()
        {
            var loadResult = Load();
            if (loadResult.IsFailure)
                return MdixResult<T>.Err(loadResult.Error);

            using var db = loadResult.SuccessResult;
            return MdixBinder.Create<T>(db, dataPath);
        }

        // Called by MdixImporter — not public API.
        public void SetData(string rawSource, string projectRelativePath)
        {
            _rawSource            = rawSource            ?? string.Empty;
            _projectRelativePath  = projectRelativePath  ?? string.Empty;
        }
    }
}
