using System;
using System.IO;
using UnityEditor;
using UnityEditor.AssetImporters;
using UnityEngine;

namespace MidManStudio.Mdix.Unity.Editor
{
    /// <summary>
    /// ScriptedImporter for .key files, so a .mdix.key shows up with its own red KEY icon.
    ///
    /// It never opens the file. The key is not copied into the imported asset, so it cannot leak
    /// into the Library cache or a build through this path.
    ///
    /// Note: this claims the ".key" extension for the project. A .key file that is not a
    /// ".mdix.key" is imported as the same empty stand-in object (the file itself is untouched).
    /// </summary>
    [ScriptedImporter(version: 1, ext: "key")]
    public sealed class MdixKeyImporter : ScriptedImporter
    {
        public override void OnImportAsset(AssetImportContext ctx)
        {
            var asset  = ScriptableObject.CreateInstance<MdixKeyAsset>();
            asset.name = Path.GetFileName(ctx.assetPath);

            ctx.AddObjectToAsset("MdixKey", asset, MdixIcons.Key);
            ctx.SetMainObject(asset);
        }
    }

    /// <summary>Inspector for a .mdix.key: what it is, the rules, and two safe actions.</summary>
    [CustomEditor(typeof(MdixKeyAsset))]
    public sealed class MdixKeyAssetEditor : UnityEditor.Editor
    {
        public override void OnInspectorGUI()
        {
            var path   = AssetDatabase.GetAssetPath(target);
            var isMdix = path.EndsWith(".mdix.key", StringComparison.OrdinalIgnoreCase);

            // Importer-made objects are NotEditable, which greys the whole Inspector out. These
            // buttons only read the file, so turn the GUI back on for them.
            var wasEnabled = GUI.enabled;
            GUI.enabled = true;
            try
            {
                EditorGUILayout.Space(4);

                if (!isMdix)
                {
                    EditorGUILayout.HelpBox(
                        "This is a .key file but not an MDIX key (those end in .mdix.key). " +
                        "MDIX does not do anything with it.",
                        MessageType.Info);
                    return;
                }

                var stem = Path.GetFileName(path);
                stem = stem.Substring(0, stem.Length - ".mdix.key".Length);

                EditorGUILayout.HelpBox(
                    "SECRET. This is the decryption key for " + stem + ".mdix.enc.\n\n" +
                    "• It is gitignored: do not commit it.\n" +
                    "• It never goes into a build, and the build stops if it would.\n" +
                    "• Do not put it in Resources or StreamingAssets.\n" +
                    "• Every compile makes a new key, so keep the .enc and .key as a pair.",
                    MessageType.Warning);

                EditorGUILayout.Space(4);

                using (new EditorGUILayout.HorizontalScope())
                {
                    if (GUILayout.Button("Copy key to clipboard", GUILayout.Height(24)))
                    {
                        try
                        {
                            GUIUtility.systemCopyBuffer = File.ReadAllText(path);
                            ShowNotification(new GUIContent("Key copied"));
                        }
                        catch (Exception ex)
                        {
                            Debug.LogError("[MDIX] Could not read the key file: " + ex.Message);
                        }
                    }

                    if (GUILayout.Button("Reveal in file manager", GUILayout.Height(24)))
                        EditorUtility.RevealInFinder(path);
                }

                EditorGUILayout.Space(6);
                EditorGUILayout.LabelField("Use it from your own code", EditorStyles.boldLabel);
                EditorGUILayout.SelectableLabel(
                    "MdixKeys.Provide(\"" + stem + "\", () => /* the key text, from YOUR server or code */);",
                    EditorStyles.textArea, GUILayout.Height(34));
            }
            finally
            {
                GUI.enabled = wasEnabled;
            }
        }

        private void ShowNotification(GUIContent content)
        {
            var w = EditorWindow.focusedWindow;
            if (w != null) w.ShowNotification(content);
        }
    }
}
