using System;
using System.IO;
using System.Collections.Generic;
using MidManStudio.Mdix.Core;
using MidManStudio.Mdix.Unity;
using UnityEditor;
using UnityEngine;

namespace MidManStudio.Mdix.Localization.Editor
{
    /// <summary>
    /// MDIX Localization Studio editor window.
    /// Open via Window → MDIX → Localization Studio.
    ///
    /// Tabs:
    ///   Overview  — scan project for locale .mdix assets.
    ///   Import    — CSV / JSON → .mdix file.
    ///   Export    — .mdix → translator CSV or JSON.
    ///   Validate  — compare a locale against a reference locale.
    ///   Bake      — populate a LocaleDataAsset SO from a locale .mdix asset.
    /// </summary>
    public sealed class MdixLocalizationEditorWindow : EditorWindow
    {
        [MenuItem("Window/MDIX/Localization Studio", priority = 201)]
        public static void Open()
        {
            var w = GetWindow<MdixLocalizationEditorWindow>("MDIX Localization");
            w.minSize = new Vector2(480, 400);
        }

        // ── State ─────────────────────────────────────────────────────────────

        private int      _tab;
        private string[] _tabNames = { "Overview", "Import", "Export", "Validate", "Bake" };

        // Overview
        private List<string> _foundAssets = new List<string>();
        private Vector2      _overviewScroll;
        private bool         _overviewDirty = true;

        // Import
        private string  _importPath        = string.Empty;
        private string  _importLocaleCode  = "fr_FR";
        private string  _importOutputDir   = "Assets/Locales";
        private string  _importPreview     = string.Empty;
        private bool    _showPreview;
        private Vector2 _previewScroll;

        // Export
        private MdixAsset? _exportAsset;
        private Vector2    _exportScroll;

        // Validate
        private MdixAsset?         _valLocale;
        private MdixAsset?         _valReference;
        private MdixLocaleReport?  _valReport;
        private Vector2            _valScroll;

        // Bake
        private MdixAsset?       _bakeSource;
        private LocaleDataAsset? _bakeTarget;
        private string           _bakeStatus = string.Empty;
        private bool             _bakeIsError;

        // ── IMGUI ─────────────────────────────────────────────────────────────

        private void OnGUI()
        {
            EditorGUILayout.Space(4);
            _tab = GUILayout.Toolbar(_tab, _tabNames);
            EditorGUILayout.Space(8);

            switch (_tab)
            {
                case 0: DrawOverview();  break;
                case 1: DrawImport();    break;
                case 2: DrawExport();    break;
                case 3: DrawValidate();  break;
                case 4: DrawBake();      break;
            }
        }

        // ── Overview tab ──────────────────────────────────────────────────────

        private void DrawOverview()
        {
            EditorGUILayout.LabelField("Locale .mdix assets in project", EditorStyles.boldLabel);
            EditorGUILayout.Space(2);

            if (_overviewDirty || GUILayout.Button("Refresh", GUILayout.Width(70)))
            {
                _foundAssets.Clear();
                foreach (var guid in AssetDatabase.FindAssets("t:MdixAsset"))
                    _foundAssets.Add(AssetDatabase.GUIDToAssetPath(guid));
                _overviewDirty = false;
            }

            EditorGUILayout.Space(4);

            _overviewScroll = EditorGUILayout.BeginScrollView(_overviewScroll);

            if (_foundAssets.Count == 0)
            {
                EditorGUILayout.HelpBox(
                    "No MdixAsset files found. Create one via\n" +
                    "Assets → Create → MDIX → Blank File.",
                    MessageType.Info);
            }
            else
            {
                foreach (var path in _foundAssets)
                {
                    EditorGUILayout.BeginHorizontal();
                    EditorGUILayout.LabelField(path, GUILayout.ExpandWidth(true));
                    if (GUILayout.Button("Select", GUILayout.Width(56)))
                        Selection.activeObject = AssetDatabase.LoadAssetAtPath<MdixAsset>(path);
                    if (GUILayout.Button("Export →", GUILayout.Width(66)))
                    {
                        _exportAsset = AssetDatabase.LoadAssetAtPath<MdixAsset>(path);
                        _tab = 2;
                    }
                    EditorGUILayout.EndHorizontal();
                }
            }

            EditorGUILayout.EndScrollView();
        }

        // ── Import tab ────────────────────────────────────────────────────────

        private void DrawImport()
        {
            EditorGUILayout.LabelField("Import locale from CSV or JSON", EditorStyles.boldLabel);
            EditorGUILayout.HelpBox(
                "CSV formats accepted:\n" +
                "  • Key | Value                  (2-column)\n" +
                "  • Key | Note | Max | Value     (4-column translator format)\n" +
                "  • Key | en_US | fr_FR | ...    (multi-locale — header is locale codes)\n\n" +
                "JSON: flat object {\"ui.play\":\"Jouer\",\"plural_enemies.one\":\"1 ennemi\",...}",
                MessageType.Info);

            EditorGUILayout.Space(4);

            EditorGUILayout.BeginHorizontal();
            _importPath = EditorGUILayout.TextField("Source file", _importPath);
            if (GUILayout.Button("Browse…", GUILayout.Width(68)))
            {
                var p = EditorUtility.OpenFilePanel("Open locale file", "", "csv,json,txt");
                if (!string.IsNullOrEmpty(p)) { _importPath = p; _showPreview = false; }
            }
            EditorGUILayout.EndHorizontal();

            _importLocaleCode = EditorGUILayout.TextField("Locale code", _importLocaleCode);
            _importOutputDir  = EditorGUILayout.TextField("Output folder", _importOutputDir);

            var meta = MdixLocaleImporter.GetLocaleDefaults(_importLocaleCode);
            EditorGUILayout.LabelField(
                $"Auto-detected: {meta.DisplayName}  ·  {meta.PluralRule}  ·  {meta.ScriptDir}",
                EditorStyles.miniLabel);

            EditorGUILayout.Space(6);

            bool fileExists = File.Exists(_importPath);
            EditorGUI.BeginDisabledGroup(!fileExists);

            EditorGUILayout.BeginHorizontal();

            if (GUILayout.Button("Preview"))
            {
                try
                {
                    var text = File.ReadAllText(_importPath);
                    var ext  = Path.GetExtension(_importPath).ToLowerInvariant();
                    _importPreview = ext == ".json"
                        ? MdixLocaleImporter.FromJson(text, _importLocaleCode)
                        : MdixLocaleImporter.FromCsv(text, _importLocaleCode);
                    _showPreview = true;
                }
                catch (Exception ex) { EditorUtility.DisplayDialog("Preview Error", ex.Message, "OK"); }
            }

            if (GUILayout.Button("Import and Save"))
                RunImport();

            EditorGUILayout.EndHorizontal();
            EditorGUI.EndDisabledGroup();

            if (!fileExists && !string.IsNullOrEmpty(_importPath))
                EditorGUILayout.HelpBox("File not found.", MessageType.Warning);

            if (_showPreview && !string.IsNullOrEmpty(_importPreview))
            {
                EditorGUILayout.Space(4);
                EditorGUILayout.LabelField("Preview (first 60 lines):", EditorStyles.boldLabel);
                var lines  = _importPreview.Split('\n');
                var capped = string.Join("\n", lines, 0, Math.Min(60, lines.Length));
                _previewScroll = EditorGUILayout.BeginScrollView(_previewScroll, GUILayout.Height(220));
                EditorGUILayout.TextArea(capped, new GUIStyle(EditorStyles.textArea)
                    { fontSize = 10, wordWrap = false },
                    GUILayout.ExpandHeight(true));
                EditorGUILayout.EndScrollView();
            }
        }

        private void RunImport()
        {
            try
            {
                var text = File.ReadAllText(_importPath);
                var ext  = Path.GetExtension(_importPath).ToLowerInvariant();

                if (ext == ".json")
                {
                    var src   = MdixLocaleImporter.FromJson(text, _importLocaleCode);
                    var path  = Path.Combine(_importOutputDir, $"{_importLocaleCode}.mdix");
                    var asset = MdixLocaleImporter.WriteAndImport(src, path);
                    if (asset != null) { EditorUtility.FocusProjectWindow(); Selection.activeObject = asset; }
                }
                else
                {
                    // Detect multi-locale CSV: header col 1 contains underscore (locale code).
                    var firstLine = text.Split(new[] { '\r', '\n' }, 2,
                        StringSplitOptions.RemoveEmptyEntries);
                    var cols   = firstLine.Length > 0 ? firstLine[0].Split(',') : Array.Empty<string>();
                    bool multi = cols.Length >= 3 && cols.Length > 1 && cols[1].Trim().Contains('_');

                    if (multi)
                    {
                        var results = MdixLocaleImporter.FromMultiLocaleCsv(text);
                        int count   = 0;
                        foreach (var kv in results)
                        {
                            var path = Path.Combine(_importOutputDir, $"{kv.Key}.mdix");
                            MdixLocaleImporter.WriteAndImport(kv.Value, path);
                            count++;
                        }
                        EditorUtility.DisplayDialog("Import Complete",
                            $"Imported {count} locale file(s) to:\n{_importOutputDir}", "OK");
                    }
                    else
                    {
                        var src   = MdixLocaleImporter.FromCsv(text, _importLocaleCode);
                        var path  = Path.Combine(_importOutputDir, $"{_importLocaleCode}.mdix");
                        var asset = MdixLocaleImporter.WriteAndImport(src, path);
                        if (asset != null) { EditorUtility.FocusProjectWindow(); Selection.activeObject = asset; }
                    }
                }

                _showPreview = false;
                _overviewDirty = true;
            }
            catch (Exception ex)
            {
                EditorUtility.DisplayDialog("Import Error", ex.Message, "OK");
            }
        }

        // ── Export tab ────────────────────────────────────────────────────────

        private void DrawExport()
        {
            EditorGUILayout.LabelField("Export locale for translators", EditorStyles.boldLabel);
            EditorGUILayout.Space(4);

            _exportAsset = (MdixAsset?)EditorGUILayout.ObjectField(
                "Locale asset", _exportAsset, typeof(MdixAsset), false);

            EditorGUILayout.Space(6);

            EditorGUI.BeginDisabledGroup(_exportAsset == null);

            EditorGUILayout.BeginHorizontal();

            if (GUILayout.Button("Export CSV  (translator format)"))
                RunExport("csv");

            if (GUILayout.Button("Export JSON"))
                RunExport("json");

            EditorGUILayout.EndHorizontal();

            EditorGUI.EndDisabledGroup();

            if (_exportAsset == null)
                EditorGUILayout.HelpBox(
                    "Select a .mdix locale asset above, then export.", MessageType.Info);
        }

        private void RunExport(string format)
        {
            if (_exportAsset == null) return;

            var dbResult = _exportAsset.Load();
            if (dbResult.IsFailure)
            {
                EditorUtility.DisplayDialog("Export Error",
                    $"Failed to load locale: {dbResult.Error.Message}", "OK");
                return;
            }

            try
            {
                using var db = dbResult.SuccessResult;

                var (content, ext) = format == "csv"
                    ? (MdixLocaleExporter.ToCsv(db),  "csv")
                    : (MdixLocaleExporter.ToJson(db),  "json");

                var baseName = Path.GetFileNameWithoutExtension(
                    _exportAsset.ProjectRelativePath) + $"_export.{ext}";

                var outPath = EditorUtility.SaveFilePanel(
                    "Export locale", string.Empty, baseName, ext);

                if (!string.IsNullOrEmpty(outPath))
                {
                    File.WriteAllText(outPath, content,
                        new System.Text.UTF8Encoding(encoderShouldEmitUTF8Identifier: false));
                    EditorUtility.DisplayDialog("Export Complete", $"Saved to:\n{outPath}", "OK");
                }
            }
            catch (Exception ex)
            {
                EditorUtility.DisplayDialog("Export Error", ex.Message, "OK");
            }
        }

        // ── Validate tab ──────────────────────────────────────────────────────

        private void DrawValidate()
        {
            EditorGUILayout.LabelField("Validate locale against reference", EditorStyles.boldLabel);
            EditorGUILayout.Space(4);

            _valLocale = (MdixAsset?)EditorGUILayout.ObjectField(
                "Locale to validate", _valLocale, typeof(MdixAsset), false);

            _valReference = (MdixAsset?)EditorGUILayout.ObjectField(
                "Reference locale",   _valReference, typeof(MdixAsset), false);

            EditorGUILayout.Space(6);

            EditorGUI.BeginDisabledGroup(_valLocale == null || _valReference == null);

            if (GUILayout.Button("Validate"))
                RunValidation();

            EditorGUI.EndDisabledGroup();

            if (_valReport == null) return;

            EditorGUILayout.Space(6);

            if (_valReport.IsValid)
            {
                EditorGUILayout.HelpBox("Validation passed — no issues found.", MessageType.None);
                return;
            }

            var summary = $"{_valReport.MissingCount} missing  ·  " +
                          $"{_valReport.EmptyCount} empty  ·  " +
                          $"{_valReport.OverLimitCount} over limit";
            EditorGUILayout.LabelField(summary, EditorStyles.boldLabel);

            _valScroll = EditorGUILayout.BeginScrollView(_valScroll);
            foreach (var issue in _valReport.Issues)
            {
                var msgType = issue.Kind == MdixLocaleIssueKind.OverLimit
                    ? MessageType.Error
                    : MessageType.Warning;
                EditorGUILayout.HelpBox($"[{issue.Kind}]  {issue.Key}\n{issue.Detail}", msgType);
            }
            EditorGUILayout.EndScrollView();
        }

        private void RunValidation()
        {
            if (_valLocale == null || _valReference == null) return;

            var locR = _valLocale.Load();
            var refR = _valReference.Load();

            if (locR.IsFailure)
            {
                EditorUtility.DisplayDialog("Error",
                    $"Failed to load locale: {locR.Error.Message}", "OK");
                return;
            }
            if (refR.IsFailure)
            {
                locR.SuccessResult.Dispose();
                EditorUtility.DisplayDialog("Error",
                    $"Failed to load reference: {refR.Error.Message}", "OK");
                return;
            }

            using var locDb = locR.SuccessResult;
            using var refDb = refR.SuccessResult;

            _valReport = MdixLocaleValidator.Validate(locDb, refDb);
        }

        // ── Bake tab ──────────────────────────────────────────────────────────

        private void DrawBake()
        {
            EditorGUILayout.LabelField("Bake locale into ScriptableObject", EditorStyles.boldLabel);
            EditorGUILayout.HelpBox(
                "Baking populates a LocaleDataAsset from a .mdix locale file.\n" +
                "Assign the resulting .asset to LocaleEntry.BakedAsset in the Inspector\n" +
                "to enable the zero-FFI runtime path in shipped / WebGL builds.",
                MessageType.Info);

            EditorGUILayout.Space(4);

            _bakeSource = (MdixAsset?)EditorGUILayout.ObjectField(
                "Source locale (.mdix)", _bakeSource, typeof(MdixAsset), false);

            _bakeTarget = (LocaleDataAsset?)EditorGUILayout.ObjectField(
                "Target asset (.asset)",  _bakeTarget, typeof(LocaleDataAsset), false);

            EditorGUILayout.Space(4);

            if (_bakeTarget == null)
            {
                EditorGUILayout.LabelField(
                    "No target: a new LocaleDataAsset will be created alongside the source.",
                    EditorStyles.miniLabel);
            }

            EditorGUILayout.Space(6);

            EditorGUI.BeginDisabledGroup(_bakeSource == null);

            if (GUILayout.Button("Bake", GUILayout.Height(28)))
                RunBake();

            EditorGUI.EndDisabledGroup();

            if (!string.IsNullOrEmpty(_bakeStatus))
            {
                EditorGUILayout.Space(4);
                EditorGUILayout.HelpBox(
                    _bakeStatus,
                    _bakeIsError ? MessageType.Error : MessageType.Info);
            }
        }

        private void RunBake()
        {
            if (_bakeSource == null) return;

            var dbResult = _bakeSource.Load();
            if (dbResult.IsFailure)
            {
                _bakeStatus  = $"Failed to load locale: {dbResult.Error.Message}";
                _bakeIsError = true;
                return;
            }

            try
            {
                using var db = dbResult.SuccessResult;

                // Resolve or create the target asset.
                var target = _bakeTarget;
                if (target == null)
                {
                    var srcDir  = Path.GetDirectoryName(_bakeSource.ProjectRelativePath) ?? "Assets";
                    var srcName = Path.GetFileNameWithoutExtension(_bakeSource.ProjectRelativePath);
                    var outPath = AssetDatabase.GenerateUniqueAssetPath(
                        $"{srcDir}/{srcName}_Baked.asset");
                    target = ScriptableObject.CreateInstance<LocaleDataAsset>();
                    AssetDatabase.CreateAsset(target, outPath);
                }

                // Populate identity and grammar metadata.
                target.LocaleCode  = db.GetString("locale_display_name")
                                       .UnwrapOr(_bakeSource.name);
                target.DisplayName = db.GetString("locale_display_name")
                                       .UnwrapOr(target.LocaleCode);
                target.Bcp47       = db.GetString("locale_bcp47")
                                       .UnwrapOr(target.LocaleCode);

                // Try enum field first (hand-written locale), fall back to string (imported locale).
                target.PluralRule   = ReadEnumOrString(db, "locale_plural_rule", "ONE_OTHER");
                target.ScriptDir    = ReadEnumOrString(db, "locale_script_dir",  "LTR");
                target.GenderSystem = ReadEnumOrString(db, "locale_gender_sys",  "NONE");

                target.DecimalSep   = db.GetString("fmt.decimal_sep").UnwrapOr(".");
                target.ThousandsSep = db.GetString("fmt.thousands_sep").UnwrapOr(",");
                target.DatePattern  = db.GetString("fmt.date_pattern").UnwrapOr("MM/DD/YYYY");

                // Collect string entries and plural entries.
                var stringEntries = new List<LocaleStringEntry>();
                var pluralEntries = new List<LocalePluralEntry>();
                var visited       = new HashSet<string>(StringComparer.Ordinal);

                BakeCollect(db, string.Empty, stringEntries, pluralEntries, visited, depth: 0);

                target.Entries       = stringEntries.ToArray();
                target.PluralEntries = pluralEntries.ToArray();

                EditorUtility.SetDirty(target);
                AssetDatabase.SaveAssets();
                AssetDatabase.Refresh();

                _bakeTarget  = target;
                _bakeStatus  = $"Baked {stringEntries.Count} string entries and " +
                               $"{pluralEntries.Count} plural entries into '{AssetDatabase.GetAssetPath(target)}'.";
                _bakeIsError = false;

                Selection.activeObject = target;
            }
            catch (Exception ex)
            {
                _bakeStatus  = $"Bake error: {ex.Message}";
                _bakeIsError = true;
            }
        }

        // Recursively walks the locale database and populates string / plural entry lists.
        private static void BakeCollect(
            MdixDatabase         db,
            string               prefix,
            List<LocaleStringEntry>  strings,
            List<LocalePluralEntry>  plurals,
            HashSet<string>      visited,
            int                  depth)
        {
            if (depth > 8) return;

            var children = db.GetKeys(prefix).UnwrapOr(Array.Empty<string>());

            foreach (var child in children)
            {
                var fullKey = string.IsNullOrEmpty(prefix) ? child : $"{prefix}.{child}";
                if (visited.Contains(fullKey) || fullKey.Contains('[')) continue;

                var vt = db.GetValueType(fullKey);

                // Recurse into objects.
                if (vt == MdixValueType.Object)
                {
                    // Detect plural entry: object with child "one" or "other".
                    if (db.Exists($"{fullKey}.one") || db.Exists($"{fullKey}.other"))
                    {
                        plurals.Add(new LocalePluralEntry
                        {
                            Key   = fullKey,
                            Zero  = db.GetString($"{fullKey}.zero").UnwrapOr(string.Empty),
                            One   = db.GetString($"{fullKey}.one").UnwrapOr(string.Empty),
                            Two   = db.GetString($"{fullKey}.two").UnwrapOr(string.Empty),
                            Few   = db.GetString($"{fullKey}.few").UnwrapOr(string.Empty),
                            Many  = db.GetString($"{fullKey}.many").UnwrapOr(string.Empty),
                            Other = db.GetString($"{fullKey}.other").UnwrapOr(string.Empty),
                        });
                        MarkSubKeysVisited(fullKey,
                            new[] { "zero","one","two","few","many","other" }, visited);
                    }
                    // Detect annotated key (key() quickfunc): object with child "value".
                    else if (db.Exists($"{fullKey}.value"))
                    {
                        var val = db.GetString($"{fullKey}.value").UnwrapOr(string.Empty);
                        strings.Add(new LocaleStringEntry { Key = fullKey, Value = val });
                        MarkSubKeysVisited(fullKey,
                            new[] { "value","note","max_chars","valid","warning" }, visited);
                    }
                    else
                    {
                        BakeCollect(db, fullKey, strings, plurals, visited, depth + 1);
                    }

                    visited.Add(fullKey);
                    continue;
                }

                if (vt == MdixValueType.Array) { visited.Add(fullKey); continue; }

                // Leaf: collect as string entry.
                var str = db.GetString(fullKey);
                if (str.IsSuccess)
                    strings.Add(new LocaleStringEntry { Key = fullKey, Value = str.SuccessResult });
                else if (vt == MdixValueType.Enum)
                {
                    var field = db.GetEnumField(fullKey).UnwrapOr(string.Empty);
                    strings.Add(new LocaleStringEntry { Key = fullKey, Value = field });
                }

                visited.Add(fullKey);
            }
        }

        private static void MarkSubKeysVisited(
            string prefix, string[] subKeys, HashSet<string> visited)
        {
            foreach (var s in subKeys) visited.Add($"{prefix}.{s}");
        }

        private static string ReadEnumOrString(MdixDatabase db, string path, string defaultValue)
        {
            var e = db.GetEnumField(path);
            if (e.IsSuccess) return e.SuccessResult;
            var s = db.GetString(path);
            return s.IsSuccess ? s.SuccessResult : defaultValue;
        }
    }
}
