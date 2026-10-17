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
    ///   [Serializable]
    ///   public class EnemyConfig { public string Name; public int Health; }
    ///
    ///   [MdixBakeable]
    ///   public class EnemyDatabase : ScriptableObject
    ///   {
    ///       public int SpawnCap;                         // reads the key spawn_cap
    ///       public List&lt;EnemyConfig&gt; Enemies;       // reads the array enemies
    ///   }
    ///
    /// The members are what Unity itself stores: public fields and [SerializeField] fields.
    /// A key in the data is matched to a member whether the member is written SpawnCap, spawnCap
    /// or spawn_cap, and a nested class must be [Serializable] or Unity drops it on save. Lists,
    /// arrays and nested objects work to any depth. Enums are matched by their number, so keep
    /// the numbers of a C# enum equal to the ones in the file's @ENUMS section.
    ///
    /// The bake reports every member the data does not fill and every key nothing reads, so a
    /// half-empty asset cannot happen without a message.
    ///
    /// The optional dataPath parameter names the @DATA object the class describes. Leave it empty
    /// when the class describes the whole file. For a class that describes one table, say "server",
    /// its members are read from server.host, server.port and so on.
    /// </summary>
    [AttributeUsage(AttributeTargets.Class, AllowMultiple = false, Inherited = false)]
    public sealed class MdixBakeableAttribute : Attribute
    {
        /// <summary>
        /// The dotted @DATA path of the object this class describes, e.g. "server" or "server.config".
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
