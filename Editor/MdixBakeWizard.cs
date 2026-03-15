using System;
using System.Collections.Generic;
using System.Linq;
using System.Reflection;
using UnityEditor;
using UnityEngine;
using MidManStudio.Mdix.Core;
using MidManStudio.Mdix.Unity;

namespace MidManStudio.Mdix.Unity.Editor
{
    /// <summary>
    /// Wizard dialog for baking a .mdix asset into a typed ScriptableObject.
    ///
    /// Flow:
    ///   Right-click .mdix asset → "Generate ScriptableObject"
    ///   → MdixBakeWizard opens
    ///   → User picks a [MdixBakeable] ScriptableObject subclass
    ///   → Wizard deserializes the mdix data into that type
    ///   → Saves result as a .asset file alongside the .mdix file
    /// </summary>
    public sealed class MdixBakeWizard : EditorWindow
    {
        // ── State ─────────────────────────────────────────────────────────────

        private MdixAsset?            _sourceAsset;
        private BakeableTypeInfo[]    _availableTypes  = Array.Empty<BakeableTypeInfo>();
        private int                   _selectedIndex;
        private string                _outputFileName  = string.Empty;
        private string                _statusMessage   = string.Empty;
        private bool                  _statusIsError;
        private Vector2               _scrollPosition;
        private string                _searchFilter    = string.Empty;
        private BakeableTypeInfo[]    _filteredTypes   = Array.Empty<BakeableTypeInfo>();

        // ── Entry point ───────────────────────────────────────────────────────

        public static void Open(MdixAsset asset)
        {
            if (asset == null)
            {
                EditorUtility.DisplayDialog(
                    "MDIX Bake Wizard",
                    "No MdixAsset selected.",
                    "OK");
                return;
            }

            var window = GetWindow<MdixBakeWizard>(
                utility: true,
                title:   "Generate ScriptableObject",
                focus:   true);

            window.minSize         = new Vector2(480, 420);
            window.maxSize         = new Vector2(480, 600);
            window._sourceAsset    = asset;
            window._outputFileName = System.IO.Path.GetFileNameWithoutExtension(
                asset.ProjectRelativePath) + "_data";

            window.RefreshTypes();
        }

        // ── Type discovery ────────────────────────────────────────────────────

        private void RefreshTypes()
        {
            var results = new List<BakeableTypeInfo>();

            foreach (var assembly in AppDomain.CurrentDomain.GetAssemblies())
            {
                // Skip Unity engine assemblies and system assemblies.
                var name = assembly.GetName().Name ?? string.Empty;
                if (name.StartsWith("Unity", StringComparison.Ordinal))    continue;
                if (name.StartsWith("System", StringComparison.Ordinal))   continue;
                if (name.StartsWith("mscorlib", StringComparison.Ordinal)) continue;
                if (name.StartsWith("Mono.", StringComparison.Ordinal))    continue;

                Type[] types;
                try   { types = assembly.GetTypes(); }
                catch { continue; }

                foreach (var type in types)
                {
                    if (!type.IsClass || type.IsAbstract)  continue;
                    if (!typeof(ScriptableObject).IsAssignableFrom(type)) continue;

                    var attr = type.GetCustomAttribute<MdixBakeableAttribute>();
                    if (attr == null) continue;

                    var displayName = string.IsNullOrEmpty(attr.DisplayName)
                        ? type.Name
                        : attr.DisplayName;

                    results.Add(new BakeableTypeInfo(
                        type,
                        displayName,
                        attr.DataPath,
                        assembly.GetName().Name ?? string.Empty));
                }
            }

            _availableTypes = results
                .OrderBy(t => t.DisplayName)
                .ToArray();

            ApplyFilter();

            if (_availableTypes.Length == 0)
            {
                _statusMessage = "No [MdixBakeable] ScriptableObject types found in the project.\n" +
                                 "Add [MdixBakeable] to a ScriptableObject subclass first.";
                _statusIsError = true;
            }
        }

        private void ApplyFilter()
        {
            _filteredTypes = string.IsNullOrEmpty(_searchFilter)
                ? _availableTypes
                : _availableTypes
                    .Where(t =>
                        t.DisplayName.IndexOf(
                            _searchFilter,
                            StringComparison.OrdinalIgnoreCase) >= 0 ||
                        t.Type.Name.IndexOf(
                            _searchFilter,
                            StringComparison.OrdinalIgnoreCase) >= 0)
                    .ToArray();

            _selectedIndex = 0;
        }

        // ── GUI ───────────────────────────────────────────────────────────────

        private void OnGUI()
        {
            DrawHeader();
            DrawTypeList();
            DrawOutputConfig();
            DrawStatusBar();
            DrawActionButtons();
        }

        private void DrawHeader()
        {
            EditorGUILayout.Space(8);

            using (new EditorGUILayout.HorizontalScope())
            {
                GUILayout.Space(10);
                EditorGUILayout.LabelField(
                    $"Source:  {_sourceAsset?.name ?? "none"}",
                    EditorStyles.boldLabel);
            }

            EditorGUILayout.Space(4);

            using (new EditorGUILayout.HorizontalScope())
            {
                GUILayout.Space(10);
                EditorGUILayout.LabelField(
                    "Pick a [MdixBakeable] type to bake this asset into:",
                    EditorStyles.wordWrappedLabel);
                GUILayout.Space(10);
            }

            EditorGUILayout.Space(6);

            // Search filter
            using (new EditorGUILayout.HorizontalScope())
            {
                GUILayout.Space(10);
                EditorGUI.BeginChangeCheck();
                _searchFilter = EditorGUILayout.TextField(
                    GUIContent.none, _searchFilter,
                    EditorStyles.toolbarSearchField,
                    GUILayout.ExpandWidth(true));
                if (EditorGUI.EndChangeCheck())
                    ApplyFilter();

                if (GUILayout.Button("↺", GUILayout.Width(26)))
                {
                    RefreshTypes();
                    _searchFilter = string.Empty;
                }
                GUILayout.Space(10);
            }

            EditorGUILayout.Space(4);
        }

        private void DrawTypeList()
        {
            if (_filteredTypes.Length == 0)
            {
                EditorGUILayout.HelpBox(
                    _availableTypes.Length == 0
                        ? "No [MdixBakeable] types found. Create a ScriptableObject " +
                          "subclass and add [MdixBakeable] to it."
                        : "No types match the search filter.",
                    MessageType.Info);
                return;
            }

            using (var scroll = new EditorGUILayout.ScrollViewScope(
                _scrollPosition,
                GUILayout.Height(180)))
            {
                _scrollPosition = scroll.scrollPosition;

                for (int i = 0; i < _filteredTypes.Length; i++)
                {
                    var info     = _filteredTypes[i];
                    var isSelected = i == _selectedIndex;

                    var style = new GUIStyle(EditorStyles.label)
                    {
                        padding  = new RectOffset(8, 8, 4, 4),
                        richText = true,
                    };

                    if (isSelected)
                    {
                        var prev = GUI.backgroundColor;
                        GUI.backgroundColor = new Color(0.23f, 0.49f, 0.97f, 0.4f);
                        EditorGUILayout.BeginHorizontal(EditorStyles.helpBox);
                        GUI.backgroundColor = prev;
                    }
                    else
                    {
                        EditorGUILayout.BeginHorizontal();
                    }

                    var label = isSelected
                        ? $"<b>{info.DisplayName}</b>  " +
                          $"<color=#7A98C4><size=10>{info.Type.FullName}</size></color>"
                        : $"{info.DisplayName}  " +
                          $"<color=#7A98C4><size=10>{info.Type.FullName}</size></color>";

                    if (GUILayout.Button(
                        new GUIContent(label),
                        style,
                        GUILayout.ExpandWidth(true)))
                    {
                        _selectedIndex = i;
                        GUI.FocusControl(null);
                    }

                    EditorGUILayout.EndHorizontal();
                }
            }

            // Show selected type detail
            if (_selectedIndex < _filteredTypes.Length)
            {
                var selected = _filteredTypes[_selectedIndex];
                EditorGUILayout.Space(4);

                using (new EditorGUILayout.HorizontalScope())
                {
                    GUILayout.Space(10);
                    var dataPathLabel = string.IsNullOrEmpty(selected.DataPath)
                        ? "root DATA section"
                        : $"@DATA path: \"{selected.DataPath}\"";

                    EditorGUILayout.LabelField(
                        $"Assembly: {selected.AssemblyName}    {dataPathLabel}",
                        EditorStyles.miniLabel);
                    GUILayout.Space(10);
                }
            }
        }

        private void DrawOutputConfig()
        {
            EditorGUILayout.Space(8);
            EditorGUILayout.LabelField("Output", EditorStyles.boldLabel);
            EditorGUILayout.Space(2);

            using (new EditorGUILayout.HorizontalScope())
            {
                EditorGUILayout.PrefixLabel("File Name");
                _outputFileName = EditorGUILayout.TextField(_outputFileName);
                EditorGUILayout.LabelField(".asset", GUILayout.Width(42));
            }

            // Show output path preview
            var outputDir = _sourceAsset != null
                ? System.IO.Path.GetDirectoryName(_sourceAsset.ProjectRelativePath)
                : "Assets";

            EditorGUILayout.LabelField(
                $"→  {outputDir}/{_outputFileName}.asset",
                EditorStyles.miniLabel);

            EditorGUILayout.Space(6);
        }

        private void DrawStatusBar()
        {
            if (string.IsNullOrEmpty(_statusMessage)) return;

            EditorGUILayout.HelpBox(
                _statusMessage,
                _statusIsError ? MessageType.Error : MessageType.Info);
        }

        private void DrawActionButtons()
        {
            GUILayout.FlexibleSpace();

            EditorGUILayout.LabelField(
                string.Empty,
                GUI.skin.horizontalSlider);

            using (new EditorGUILayout.HorizontalScope())
            {
                GUILayout.Space(10);

                if (GUILayout.Button("Cancel", GUILayout.Height(28), GUILayout.Width(80)))
                    Close();

                GUILayout.FlexibleSpace();

                var canBake = _sourceAsset != null
                    && _filteredTypes.Length > 0
                    && _selectedIndex < _filteredTypes.Length
                    && !string.IsNullOrWhiteSpace(_outputFileName);

                using (new EditorGUI.DisabledScope(!canBake))
                {
                    if (GUILayout.Button(
                        "Generate ScriptableObject",
                        GUILayout.Height(28),
                        GUILayout.Width(200)))
                    {
                        TryBake();
                    }
                }

                GUILayout.Space(10);
            }

            EditorGUILayout.Space(8);
        }

        // ── Bake ──────────────────────────────────────────────────────────────

        private void TryBake()
        {
            if (_sourceAsset == null || _selectedIndex >= _filteredTypes.Length)
                return;

            var typeInfo = _filteredTypes[_selectedIndex];

            // Load the mdix data.
            var loadResult = _sourceAsset.Load();
            if (loadResult.IsFailure)
            {
                _statusMessage = $"Parse failed: {loadResult.Error.Message}";
                _statusIsError = true;
                return;
            }

            using var db = loadResult.SuccessResult;

            // Deserialize into the target type via reflection — the serializer
            // handles all the property mapping exactly as it would for a plain POCO.
            object? instance;
            try
            {
                instance = ScriptableObject.CreateInstance(typeInfo.Type);

                var prefix = string.IsNullOrEmpty(typeInfo.DataPath)
                    ? null
                    : typeInfo.DataPath;

                // Use MdixSerializer via the database Deserialize path.
                // We need to call the generic method via reflection because
                // the type is only known at runtime.
                var deserializeMethod = typeof(MdixDatabase)
                    .GetMethod(nameof(MdixDatabase.Deserialize))!
                    .MakeGenericMethod(typeInfo.Type);

                var result = deserializeMethod.Invoke(
                    db, new object?[] { prefix });

                // result is MdixResult<T> — check IsSuccess via reflection.
                var resultType  = result!.GetType();
                var isSuccess   = (bool)resultType
                    .GetProperty("IsSuccess")!
                    .GetValue(result)!;

                if (!isSuccess)
                {
                    var error = resultType
                        .GetProperty("Error")!
                        .GetValue(result)!
                        .ToString();

                    _statusMessage = $"Deserialization failed: {error}";
                    _statusIsError = true;

                    DestroyImmediate((ScriptableObject)instance);
                    return;
                }

                var deserialized = resultType
                    .GetProperty("SuccessResult")!
                    .GetValue(result)!;

                // Copy deserialized property values onto the ScriptableObject.
                // We instantiated the SO first so Unity serialization works —
                // now we copy all public properties from the deserialized POCO.
                foreach (var prop in typeInfo.Type.GetProperties(
                    BindingFlags.Public | BindingFlags.Instance))
                {
                    if (!prop.CanWrite || !prop.CanRead) continue;
                    try
                    {
                        prop.SetValue(instance, prop.GetValue(deserialized));
                    }
                    catch { /* property may not be serializable — skip */ }
                }
            }
            catch (Exception ex)
            {
                _statusMessage = $"Bake error: {ex.Message}";
                _statusIsError = true;
                return;
            }

            // Save as .asset file.
            var outputDir  = System.IO.Path.GetDirectoryName(
                _sourceAsset.ProjectRelativePath) ?? "Assets";
            var outputPath = $"{outputDir}/{_outputFileName}.asset";

            // Warn before overwrite.
            if (AssetDatabase.LoadAssetAtPath<ScriptableObject>(outputPath) != null)
            {
                if (!EditorUtility.DisplayDialog(
                    "Overwrite?",
                    $"'{outputPath}' already exists. Overwrite it?",
                    "Overwrite", "Cancel"))
                    return;
            }

            AssetDatabase.CreateAsset((ScriptableObject)instance, outputPath);
            AssetDatabase.SaveAssets();
            AssetDatabase.Refresh();

            EditorUtility.FocusProjectWindow();
            Selection.activeObject = AssetDatabase.LoadAssetAtPath<ScriptableObject>(outputPath);

            _statusMessage = $"Generated: {outputPath}";
            _statusIsError = false;

            // Auto-close after a short delay so the user sees the success message.
            EditorApplication.delayCall += Close;
        }

        // ── Data types ────────────────────────────────────────────────────────

        private sealed class BakeableTypeInfo
        {
            public Type   Type         { get; }
            public string DisplayName  { get; }
            public string DataPath     { get; }
            public string AssemblyName { get; }

            public BakeableTypeInfo(
                Type   type,
                string displayName,
                string dataPath,
                string assemblyName)
            {
                Type         = type;
                DisplayName  = displayName;
                DataPath     = dataPath;
                AssemblyName = assemblyName;
            }
        }
    }
}
