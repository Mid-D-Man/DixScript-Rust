using System;
using System.Collections.Generic;
using System.Linq;
using System.Reflection;
using System.Text;
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
    ///   Right-click .mdix asset, then "Generate ScriptableObject"
    ///   -> the wizard lists every [MdixBakeable] ScriptableObject subclass, best fit first
    ///   -> the person picks one
    ///   -> <see cref="MdixBinder"/> fills a new instance from the data
    ///   -> the result is saved as a .asset file next to the .mdix file
    ///      (an existing asset is updated in place, so references to it survive)
    ///
    /// What the binder reads: public fields and [SerializeField] fields, which is what Unity itself
    /// serializes, plus writable properties; lists, arrays, nested [Serializable] classes and enums
    /// at any depth. See <see cref="MdixBinder"/> for how keys are matched to member names.
    ///
    /// A bake that is incomplete never passes silently. Members the data does not fill, keys that
    /// nothing reads, values that could not be converted, and members Unity would not store are all
    /// listed. A bake with unconvertible values is refused, because the asset would otherwise hold
    /// defaults where the data had something else.
    /// </summary>
    public sealed class MdixBakeWizard : EditorWindow
    {
        private const int MaxListedPerGroup = 4;

        private enum StatusKind { Info, Warning, Error }

        // ── State ─────────────────────────────────────────────────────────────

        private MdixAsset          _sourceAsset;
        private BakeableTypeInfo[] _availableTypes  = Array.Empty<BakeableTypeInfo>();
        private int                _selectedIndex;
        private string             _outputFileName  = string.Empty;
        private string             _statusMessage   = string.Empty;
        private StatusKind         _statusKind      = StatusKind.Info;
        private Vector2            _scrollPosition;
        private string             _searchFilter    = string.Empty;
        private BakeableTypeInfo[] _filteredTypes   = Array.Empty<BakeableTypeInfo>();

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
            window.maxSize         = new Vector2(480, 640);
            window._sourceAsset    = asset;
            window._outputFileName = System.IO.Path.GetFileNameWithoutExtension(
                asset.ProjectRelativePath) + "_data";
            window._statusMessage  = string.Empty;
            window._statusKind     = StatusKind.Info;

            window.RefreshTypes();
        }

        private void SetStatus(StatusKind kind, string message)
        {
            _statusKind    = kind;
            _statusMessage = message;
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

            ProbeTypes(results);

            // Best fit first, so the class that matches this file is already selected.
            _availableTypes = results
                .OrderByDescending(t => t.Score)
                .ThenBy(t => t.Unused)
                .ThenBy(t => t.DisplayName, StringComparer.OrdinalIgnoreCase)
                .ToArray();

            ApplyFilter();

            if (_availableTypes.Length == 0)
            {
                SetStatus(StatusKind.Error,
                    "No [MdixBakeable] ScriptableObject types found in the project.\n" +
                    "Add [MdixBakeable] to a ScriptableObject subclass first.");
            }
        }

        /// <summary>Measures how much of this file each candidate type would read.</summary>
        private void ProbeTypes(List<BakeableTypeInfo> types)
        {
            if (_sourceAsset == null || types.Count == 0) return;

            var load = _sourceAsset.Load();
            if (load.IsFailure)
            {
                SetStatus(StatusKind.Error,
                    "This file has an error, so the types cannot be compared with it:\n" +
                    load.Error.Message);
                return;
            }

            using (var db = load.SuccessResult)
            {
                foreach (var info in types)
                {
                    try
                    {
                        var report = MdixBinder.Probe(db, info.Type, info.DataPath);
                        info.Probed   = true;
                        info.Matched  = report.MembersMatched;
                        info.Seen     = report.MembersSeen;
                        info.Unused   = report.UnusedKeys.Count;
                        info.Problems = report.Problems.Count;
                    }
                    catch (Exception ex)
                    {
                        info.ProbeError = ex.Message;
                    }
                }
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
                    "Source:  " + (_sourceAsset != null ? _sourceAsset.name : "none"),
                    EditorStyles.boldLabel);
            }

            EditorGUILayout.Space(4);

            using (new EditorGUILayout.HorizontalScope())
            {
                GUILayout.Space(10);
                EditorGUILayout.LabelField(
                    "Pick a [MdixBakeable] type to bake this asset into. The type that reads the most of this file is listed first:",
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
                    var info       = _filteredTypes[i];
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

                    var title = isSelected ? "<b>" + info.DisplayName + "</b>" : info.DisplayName;
                    var label = title + "  " +
                                "<color=#7A98C4><size=10>" + info.Type.FullName + "</size></color>" +
                                info.FitMarkup;

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
                        : "@DATA path: \"" + selected.DataPath + "\"";

                    EditorGUILayout.LabelField(
                        "Assembly: " + selected.AssemblyName + "    " + dataPathLabel,
                        EditorStyles.miniLabel);
                    GUILayout.Space(10);
                }

                var fit = selected.FitDetail;
                if (fit.Length > 0)
                {
                    using (new EditorGUILayout.HorizontalScope())
                    {
                        GUILayout.Space(10);
                        EditorGUILayout.LabelField(fit, EditorStyles.miniLabel);
                        GUILayout.Space(10);
                    }
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
                ? OutputDirectory()
                : "Assets";

            EditorGUILayout.LabelField(
                "→  " + outputDir + "/" + _outputFileName + ".asset",
                EditorStyles.miniLabel);

            EditorGUILayout.Space(6);
        }

        private void DrawStatusBar()
        {
            if (string.IsNullOrEmpty(_statusMessage)) return;

            var type = _statusKind == StatusKind.Error   ? MessageType.Error
                     : _statusKind == StatusKind.Warning ? MessageType.Warning
                     :                                     MessageType.Info;

            EditorGUILayout.HelpBox(_statusMessage, type);
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

        private string OutputDirectory()
        {
            var dir = System.IO.Path.GetDirectoryName(_sourceAsset.ProjectRelativePath);
            return string.IsNullOrEmpty(dir) ? "Assets" : dir.Replace('\\', '/');
        }

        private void TryBake()
        {
            if (_sourceAsset == null || _selectedIndex >= _filteredTypes.Length)
                return;

            var typeInfo = _filteredTypes[_selectedIndex];

            // Load the mdix data.
            var loadResult = _sourceAsset.Load();
            if (loadResult.IsFailure)
            {
                SetStatus(StatusKind.Error, "Parse failed: " + loadResult.Error.Message);
                return;
            }

            // CreateInstance, not new: a ScriptableObject cannot be built with its constructor.
            ScriptableObject instance = null;
            MdixBindReport   report;

            using (var db = loadResult.SuccessResult)
            {
                try
                {
                    instance = ScriptableObject.CreateInstance(typeInfo.Type);
                    report   = MdixBinder.Bind(db, instance, typeInfo.DataPath);
                }
                catch (Exception ex)
                {
                    if (instance != null) DestroyImmediate(instance);
                    SetStatus(StatusKind.Error, "Bake error: " + ex.Message);
                    return;
                }
            }

            if (report.NothingMatched)
            {
                DestroyImmediate(instance);
                SetStatus(StatusKind.Error,
                    "Nothing in this file fits " + typeInfo.Type.Name + ", so there is nothing to bake.\n" +
                    Describe(report));
                return;
            }

            if (report.Problems.Count > 0)
            {
                DestroyImmediate(instance);
                SetStatus(StatusKind.Error,
                    "Not baked: some values could not be converted, and the asset would hold defaults " +
                    "in their place.\n" + Describe(report));
                return;
            }

            var outputPath = OutputDirectory() + "/" + _outputFileName + ".asset";

            ScriptableObject saved;
            var existing = AssetDatabase.LoadAssetAtPath<ScriptableObject>(outputPath);

            if (existing != null)
            {
                if (existing.GetType() != typeInfo.Type)
                {
                    DestroyImmediate(instance);
                    SetStatus(StatusKind.Error,
                        "'" + outputPath + "' already exists and is a " + existing.GetType().Name +
                        ", not a " + typeInfo.Type.Name + ". Pick another file name.");
                    return;
                }

                if (!EditorUtility.DisplayDialog(
                    "Overwrite?",
                    "'" + outputPath + "' already exists. Replace its data with this bake?\n" +
                    "References to the asset are kept.",
                    "Replace", "Cancel"))
                {
                    DestroyImmediate(instance);
                    return;
                }

                // Update in place, so scenes and prefabs that point at the asset keep working.
                var keepName  = existing.name;
                var keepFlags = existing.hideFlags;
                EditorUtility.CopySerialized(instance, existing);
                existing.name      = keepName;
                existing.hideFlags = keepFlags;
                EditorUtility.SetDirty(existing);
                DestroyImmediate(instance);
                saved = existing;
            }
            else
            {
                AssetDatabase.CreateAsset(instance, outputPath);
                saved = instance;
            }

            AssetDatabase.SaveAssets();
            AssetDatabase.Refresh();

            EditorUtility.FocusProjectWindow();
            Selection.activeObject = saved;

            // Anything Unity itself would drop when it saves this type.
            var warnings = new List<string>();
            AppendGroup(warnings, "Left at their defaults, the file has no value for", report.MissingMembers);
            AppendGroup(warnings, "In the file but read by nothing", report.UnusedKeys);
            AppendGroup(warnings, "Unity will not store", MdixBinder.FindSerializationGaps(typeInfo.Type));

            var head = "Generated: " + outputPath + "\n" + report.BoundValues + " values read.";

            if (warnings.Count == 0)
            {
                SetStatus(StatusKind.Info, head);

                // Auto-close after a short delay so the person sees the success message.
                EditorApplication.delayCall += Close;
            }
            else
            {
                // Deliberately not auto-closing: the list needs to be read.
                SetStatus(StatusKind.Warning, head + "\n\n" + string.Join("\n", warnings));
            }
        }

        private static void AppendGroup(List<string> lines, string heading, List<string> items)
        {
            if (items.Count == 0) return;

            var sb = new StringBuilder();
            sb.Append(heading).Append(": ");
            sb.Append(string.Join("; ", items.Take(MaxListedPerGroup)));
            if (items.Count > MaxListedPerGroup)
                sb.Append("; and ").Append(items.Count - MaxListedPerGroup).Append(" more");

            lines.Add(sb.ToString());
        }

        /// <summary>The full picture of a refused bake, in a few lines.</summary>
        private static string Describe(MdixBindReport report)
        {
            var lines = new List<string>();
            AppendGroup(lines, "Cannot convert", report.Problems);
            AppendGroup(lines, "No value in the file for", report.MissingMembers);
            AppendGroup(lines, "In the file but read by nothing", report.UnusedKeys);
            return string.Join("\n", lines);
        }

        // ── Data types ────────────────────────────────────────────────────────

        private sealed class BakeableTypeInfo
        {
            public Type   Type         { get; }
            public string DisplayName  { get; }
            public string DataPath     { get; }
            public string AssemblyName { get; }

            // Filled by ProbeTypes: how much of the source file this type reads.
            public bool   Probed     { get; set; }
            public int    Matched    { get; set; }
            public int    Seen       { get; set; }
            public int    Unused     { get; set; }
            public int    Problems   { get; set; }
            public string ProbeError { get; set; }

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

            public bool IsPerfect =>
                Probed && Seen > 0 && Matched == Seen && Unused == 0 && Problems == 0;

            /// <summary>0 to 1: the share of this type's members that find data. Each unconvertible value costs a little.</summary>
            public float Score =>
                !Probed || Seen <= 0 ? 0f : Math.Max(0f, (float)Matched / Seen - Problems * 0.01f);

            /// <summary>Short coloured tag for the list: green when it fits exactly, amber when partly, grey when not at all.</summary>
            public string FitMarkup
            {
                get
                {
                    if (!Probed) return string.Empty;

                    var colour = IsPerfect ? "#7FD28A" : (Matched > 0 ? "#E6B450" : "#8A8F98");
                    var text   = IsPerfect ? "fits exactly" : "reads " + Matched + " of " + Seen;
                    return "  <color=" + colour + "><size=10>" + text + "</size></color>";
                }
            }

            /// <summary>One line under the list for the selected type.</summary>
            public string FitDetail
            {
                get
                {
                    if (!string.IsNullOrEmpty(ProbeError)) return "Could not compare with this file: " + ProbeError;
                    if (!Probed)                           return string.Empty;
                    if (IsPerfect)                         return "Every member of this type finds its value, and every key in the file is read.";

                    return "Reads " + Matched + " of " + Seen + " members; " + Unused + " key(s) in the file are not read" +
                           (Problems > 0 ? "; " + Problems + " value(s) cannot be converted." : ".");
                }
            }
        }
    }
}
