using System;
using System.IO;
using UnityEditor;
using UnityEditor.AssetImporters;
using UnityEngine;

namespace MidManStudio.Mdix.Unity.Editor
{
    /// <summary>
    /// ScriptedImporter for .enc files, so a .mdix.enc is a real asset with its own violet ENC icon.
    ///
    /// It stores the encrypted bytes in a MdixEncAsset, which can be dragged into an Inspector
    /// field and is then included in the build on every platform. The key is not part of it.
    ///
    /// Note: this claims the ".enc" extension for the project. An .enc file that is not a
    /// ".mdix.enc" is imported the same way (its bytes in a MdixEncAsset) and is harmless.
    /// </summary>
    [ScriptedImporter(version: 1, ext: "enc")]
    public sealed class MdixEncImporter : ScriptedImporter
    {
        public override void OnImportAsset(AssetImportContext ctx)
        {
            var bytes = File.ReadAllBytes(ctx.assetPath);

            var asset  = ScriptableObject.CreateInstance<MdixEncAsset>();
            asset.name = Path.GetFileName(ctx.assetPath);
            asset.SetData(bytes, KeyNameFor(ctx.assetPath), ctx.assetPath);

            ctx.AddObjectToAsset("MdixEncAsset", asset, MdixIcons.Enc);
            ctx.SetMainObject(asset);
        }

        /// <summary>secrets.mdix.enc → "secrets"; anything else → the name without its extension.</summary>
        internal static string KeyNameFor(string assetPath)
        {
            var name = Path.GetFileName(assetPath);
            const string suffix = ".mdix.enc";

            return name.EndsWith(suffix, StringComparison.OrdinalIgnoreCase)
                ? name.Substring(0, name.Length - suffix.Length)
                : Path.GetFileNameWithoutExtension(name);
        }
    }

    /// <summary>Inspector for a .mdix.enc: its key name, size, the matching key file, and how to load it.</summary>
    [CustomEditor(typeof(MdixEncAsset))]
    public sealed class MdixEncAssetEditor : UnityEditor.Editor
    {
        public override void OnInspectorGUI()
        {
            var asset = (MdixEncAsset)target;
            var path  = AssetDatabase.GetAssetPath(target);

            var wasEnabled = GUI.enabled;
            GUI.enabled = true;
            try
            {
                EditorGUILayout.Space(4);

                EditorGUILayout.HelpBox(
                    "Encrypted data. It is safe to ship: it cannot be read without its key, " +
                    "and the key is not in this asset. Provide the key from your own code.",
                    MessageType.Info);

                EditorGUILayout.Space(4);

                using (new EditorGUI.DisabledScope(true))
                {
                    EditorGUILayout.TextField("Key name", asset.KeyName);
                    EditorGUILayout.TextField("Size", asset.SizeBytes + " bytes");
                }

                // The key that was generated with this file, if it is still next to it.
                var keyPath = path.EndsWith(".mdix.enc", StringComparison.OrdinalIgnoreCase)
                    ? path.Substring(0, path.Length - ".mdix.enc".Length) + ".mdix.key"
                    : string.Empty;
                var keyAsset = keyPath.Length > 0
                    ? AssetDatabase.LoadMainAssetAtPath(keyPath)
                    : null;

                EditorGUILayout.Space(4);
                using (new EditorGUILayout.HorizontalScope())
                {
                    using (new EditorGUI.DisabledScope(keyAsset == null))
                    {
                        if (GUILayout.Button("Select key file", GUILayout.Height(24)))
                        {
                            Selection.activeObject = keyAsset;
                            EditorGUIUtility.PingObject(keyAsset);
                        }
                    }

                    if (GUILayout.Button("Reveal in file manager", GUILayout.Height(24)))
                        EditorUtility.RevealInFinder(path);
                }

                if (keyAsset == null)
                {
                    EditorGUILayout.HelpBox(
                        "No .mdix.key next to this file. That is fine for a build machine or a fresh " +
                        "clone (keys are not in git). Get the key from wherever you keep it.",
                        MessageType.None);
                }

                EditorGUILayout.Space(6);
                EditorGUILayout.LabelField("Load it", EditorStyles.boldLabel);
                EditorGUILayout.SelectableLabel(
                    "// once, from your own code:\n" +
                    "MdixKeys.Provide(\"" + asset.KeyName + "\", () => /* the key text */);\n\n" +
                    "// then, with this asset in a field:\n" +
                    "using var db = encAsset.Load().Unwrap();",
                    EditorStyles.textArea, GUILayout.Height(80));
            }
            finally
            {
                GUI.enabled = wasEnabled;
            }
        }
    }
}
