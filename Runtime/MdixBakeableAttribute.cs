using System;

namespace MidManStudio.Mdix.Unity
{
    /// <summary>
    /// Marks a ScriptableObject subclass as a valid bake target for the
    /// right-click "Generate ScriptableObject" workflow in the MDIX Studio editor.
    ///
    /// The class must also inherit from UnityEngine.ScriptableObject.
    ///
    /// Usage:
    ///   [MdixBakeable("enemies")]
    ///   public class EnemyDataAsset : ScriptableObject
    ///   {
    ///       public List<EnemyConfig> enemies;
    ///   }
    ///
    /// The optional dataPath parameter tells the bake wizard which @DATA path
    /// to read from. Leave it empty to read from the root.
    /// </summary>
    [AttributeUsage(AttributeTargets.Class, AllowMultiple = false, Inherited = false)]
    public sealed class MdixBakeableAttribute : Attribute
    {
        /// <summary>
        /// The dotted @DATA path this class maps to, e.g. "enemies" or "server.config".
        /// Empty string means the root DATA section.
        /// </summary>
        public string DataPath { get; }

        /// <summary>
        /// Human-readable label shown in the bake wizard type picker.
        /// Defaults to the class name if not specified.
        /// </summary>
        public string DisplayName { get; }

        public MdixBakeableAttribute(string dataPath = "", string displayName = "")
        {
            DataPath    = dataPath    ?? string.Empty;
            DisplayName = displayName ?? string.Empty;
        }
    }
}
