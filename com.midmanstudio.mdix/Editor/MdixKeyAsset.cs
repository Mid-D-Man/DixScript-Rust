using UnityEngine;

namespace MidManStudio.Mdix.Unity.Editor
{
    /// <summary>
    /// Stand-in object for a .mdix.key file in the Project window.
    ///
    /// It deliberately holds NOTHING: not the key, not a copy of the file. It exists so the key
    /// gets its own icon and Inspector. It lives in the Editor assembly, so it cannot be part of a
    /// player build, and the key itself is never serialized into any asset.
    /// </summary>
    public sealed class MdixKeyAsset : ScriptableObject
    {
    }
}
