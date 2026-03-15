using System.IO;
using UnityEditor;
using UnityEditor.AssetImporters;
using UnityEngine;
using MidManStudio.Mdix.Unity;

namespace MidManStudio.Mdix.Unity.Editor
{
    /// <summary>
    /// ScriptedImporter for .mdix files.
    /// Makes .mdix a first-class Unity asset type — shows in the Project window
    /// with a custom icon, supports drag-and-drop into Inspector fields.
    ///
    /// On import: reads the file text and stores it in a MdixAsset ScriptableObject.
    /// Parse errors are logged as import warnings but never fail the import —
    /// the raw source is always preserved so the user can fix errors in the editor.
    /// </summary>
    [ScriptedImporter(version: 1, ext: "mdix")]
    public sealed class MdixImporter : ScriptedImporter
    {
        public override void OnImportAsset(AssetImportContext ctx)
        {
            var source = File.ReadAllText(ctx.assetPath);
            var asset  = ScriptableObject.CreateInstance<MdixAsset>();

            // Store relative path (Assets/...) for runtime path resolution.
            asset.SetData(source, ctx.assetPath);

            // Validate the source and report errors as import warnings.
            // We never fail the import — bad source still produces a usable
            // MdixAsset so the editor remains responsive while the user fixes syntax.
            var validation = MidManStudio.Mdix.Dix.LoadStr(source);
            if (validation.IsFailure)
            {
                ctx.LogImportWarning(
                    $"MdixImporter: parse error in '{ctx.assetPath}': " +
                    $"{validation.Error.Message}");
            }
            else
            {
                // Dispose immediately — we only needed it for validation.
                validation.SuccessResult.Dispose();
            }

            // Register as main asset. The MdixAsset IS the imported object.
            ctx.AddObjectToAsset("MdixAsset", asset, GetIcon());
            ctx.SetMainObject(asset);
        }

        private static Texture2D? GetIcon()
        {
            // Try to load the custom icon from the Editor/Icons folder.
            // Falls back to null (Unity default icon) if it is not present.
            return AssetDatabase.LoadAssetAtPath<Texture2D>(
                "Packages/com.midmanstudio.mdix/Editor/Icons/mdix_icon.png");
        }
    }

    /// <summary>
    /// Custom Inspector for MdixAsset.
    /// Shows source preview, parse status, and action buttons.
    /// </summary>
    [CustomEditor(typeof(MdixAsset))]
    public sealed class MdixAssetEditor : UnityEditor.Editor
    {
        private bool   _showSource;
        private string _statusMessage  = string.Empty;
        private bool   _statusIsError;

        public override void OnInspectorGUI()
        {
            var asset = (MdixAsset)target;

            EditorGUILayout.Space(4);

            // ── Status row ────────────────────────────────────────────────────

            var parseResult = MidManStudio.Mdix.Dix.LoadStr(asset.RawSource);
            var isValid     = parseResult.IsSuccess;

            if (isValid)
            {
                var db         = parseResult.SuccessResult;
                var entryCount = db.EntryCount;
                db.Dispose();

                DrawStatusBadge(
                    $"✓  {entryCount} entries",
                    new Color(0.27f, 0.72f, 0.45f));
            }
            else
            {
                DrawStatusBadge(
                    $"✗  {parseResult.Error.Message}",
                    new Color(0.85f, 0.33f, 0.33f));
            }

            EditorGUILayout.Space(6);

            // ── Action buttons ────────────────────────────────────────────────

            using (new EditorGUILayout.HorizontalScope())
            {
                if (GUILayout.Button("Open in MDIX Studio", GUILayout.Height(26)))
                    MdixEditorWindow.OpenWithAsset(asset);

                if (GUILayout.Button("Generate ScriptableObject", GUILayout.Height(26)))
                    MdixBakeWizard.Open(asset);
            }

            EditorGUILayout.Space(6);

            // ── Source preview ────────────────────────────────────────────────

            _showSource = EditorGUILayout.Foldout(_showSource, "Source Preview", true);
            if (_showSource)
            {
                var style = new GUIStyle(EditorStyles.textArea)
                {
                    fontStyle = FontStyle.Normal,
                    fontSize  = 11,
                    wordWrap  = false,
                };
                EditorGUILayout.TextArea(asset.RawSource, style,
                    GUILayout.MinHeight(120), GUILayout.ExpandHeight(true));

                EditorGUILayout.HelpBox(
                    "Edit this file in MDIX Studio or any text editor. " +
                    "Unity reimports automatically on save.",
                    MessageType.Info);
            }

            // ── Path info ─────────────────────────────────────────────────────

            EditorGUILayout.Space(4);
            using (new EditorGUI.DisabledScope(true))
            {
                EditorGUILayout.TextField("Asset Path", asset.ProjectRelativePath);
            }
        }

        private static void DrawStatusBadge(string text, Color color)
        {
            var style = new GUIStyle(EditorStyles.helpBox)
            {
                fontSize  = 12,
                alignment = TextAnchor.MiddleLeft,
                padding   = new RectOffset(10, 10, 6, 6),
            };

            var prev = GUI.color;
            GUI.color = color;
            EditorGUILayout.LabelField(text, style);
            GUI.color = prev;
        }
    }
}
