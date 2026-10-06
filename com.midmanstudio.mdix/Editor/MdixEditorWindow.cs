using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using UnityEditor;
using UnityEngine;
using UnityEngine.UIElements;
using MidManStudio.Mdix.Core;
using MidManStudio.Mdix.Unity;

namespace MidManStudio.Mdix.Unity.Editor
{
    /// <summary>
    /// MDIX Studio — the main editor window.
    /// Three tabs: Explorer (compiled data viewer), Editor (source text),
    /// Templates (new file creation).
    /// Open via Window → MDIX Studio, or by double-clicking a .mdix asset.
    /// </summary>
    public sealed partial class MdixEditorWindow : EditorWindow
    {
        // ── Constants ─────────────────────────────────────────────────────────

        private const string UxmlPath =
            "Packages/com.midmanstudio.mdix/Editor/UI/MdixEditorWindow.uxml";
        private const string UssPath  =
            "Packages/com.midmanstudio.mdix/Editor/UI/MdixEditorWindow.uss";

        private const string PrefKeyLastPath = "MdixStudio_LastPath";

        // ── State ─────────────────────────────────────────────────────────────

        private MdixAsset?   _currentAsset;
        private string       _currentPath  = string.Empty;
        private string       _sourceText   = string.Empty;
        private bool         _isDirty;
        private int          _activeTab;   // 0 = Explorer, 1 = Editor, 2 = Templates

        // UI element references
        private Label?         _titleLabel;
        private Label?         _fileLabel;
        private Label?         _statusLabel;
        private VisualElement? _panelExplorer;
        private VisualElement? _panelEditor;
        private VisualElement? _panelTemplates;
        private TextField?     _codeField;
        private Label?         _sbCompiled;
        private Label?         _sbEntries;
        private Label?         _sbFlatKeys;
        private Label?         _sbTables;
        private Label?         _sbPath;

        private VisualElement? _tabExplorer;
        private VisualElement? _tabEditor;
        private VisualElement? _tabTemplates;
        private Button?        _btnFoldAll;

        // Explorer folding. The titles of collapsed sections are remembered across rebuilds,
        // so the layout survives every recompile instead of snapping back open.
        private readonly HashSet<string>       _foldedSections = new HashSet<string>();
        private readonly List<ExplorerSection> _sections       = new List<ExplorerSection>();

        private readonly struct ExplorerSection
        {
            public readonly string        Title;
            public readonly Label         Header;
            public readonly VisualElement Body;

            public ExplorerSection(string title, Label header, VisualElement body)
            {
                Title  = title;
                Header = header;
                Body   = body;
            }
        }

        // ── Menu and entry points ─────────────────────────────────────────────

        [MenuItem("MidManStudio/MDIX Studio")]
        public static void Open()
        {
            var window = GetWindow<MdixEditorWindow>("MDIX Studio");
            window.minSize = new Vector2(600, 480);

            var lastPath = EditorPrefs.GetString(PrefKeyLastPath, string.Empty);
            if (!string.IsNullOrEmpty(lastPath))
            {
                // The menu item only brings the window forward: it must never replace edits
                // that haven't been saved.
                var asset = AssetDatabase.LoadAssetAtPath<MdixAsset>(lastPath);
                if (asset != null && !window._isDirty)
                    window.LoadAsset(asset);
            }
        }

        public static void OpenWithAsset(MdixAsset asset)
        {
            var window = GetWindow<MdixEditorWindow>("MDIX Studio");
            window.minSize = new Vector2(600, 480);

            if (window.ConfirmLeaveCurrentDocument(asset.ProjectRelativePath))
                window.LoadAsset(asset);
        }

        /// <summary>
        /// Double-clicking a .mdix asset opens it here (the README has always promised this).
        /// Returns false for anything else, so Unity's normal handling is left alone.
        /// </summary>
        [UnityEditor.Callbacks.OnOpenAsset(1)]
        public static bool OnOpenMdixAsset(int instanceID, int line)
        {
            var asset = EditorUtility.InstanceIDToObject(instanceID) as MdixAsset;
            if (asset == null) return false;

            OpenWithAsset(asset);
            return true;
        }

        /// <summary>
        /// True if it is fine to replace the document on screen with <paramref name="nextPath"/>.
        /// Opening another file used to overwrite unsaved edits without a word.
        /// </summary>
        private bool ConfirmLeaveCurrentDocument(string nextPath)
        {
            if (!_isDirty) return true;

            // Asked to open the file that is already open and edited: keep the edits, just show the window.
            if (string.Equals(_currentPath, nextPath, StringComparison.Ordinal)) return false;

            var scratch = string.IsNullOrEmpty(_currentPath);
            var name    = scratch ? "Untitled" : Path.GetFileName(_currentPath);

            var choice = EditorUtility.DisplayDialogComplex(
                "Unsaved Changes",
                $"'{name}' has unsaved changes. Save before opening '{Path.GetFileName(nextPath)}'?",
                "Save", "Cancel", "Discard");

            switch (choice)
            {
                case 0: // Save
                    if (scratch)
                        return PromptAndWriteNewFile() != null; // cancelled panel => stay put

                    SaveSource();
                    return !_isDirty;                           // a failed save => stay put

                case 2: // Discard
                    return true;

                default: // Cancel
                    return false;
            }
        }

        // ── UIElements lifecycle ──────────────────────────────────────────────

        public void CreateGUI()
        {
            var uxml = AssetDatabase.LoadAssetAtPath<VisualTreeAsset>(UxmlPath);
            if (uxml == null)
            {
                rootVisualElement.Add(new Label(
                    "MDIX Studio: UXML not found. " +
                    "Ensure the package is correctly installed."));
                return;
            }

            uxml.CloneTree(rootVisualElement);

            var uss = AssetDatabase.LoadAssetAtPath<StyleSheet>(UssPath);
            if (uss != null)
                rootVisualElement.styleSheets.Add(uss);

            BindElements();
            BuildTemplatesPanel();
            ShowTab(0);
        }

        private void BindElements()
        {
            var root = rootVisualElement;

            _titleLabel     = root.Q<Label>("title-label");
            _fileLabel      = root.Q<Label>("file-label");
            _statusLabel    = root.Q<Label>("status-label");
            _panelExplorer  = root.Q("panel-explorer");
            _panelEditor    = root.Q("panel-editor");
            _panelTemplates = root.Q("panel-templates");
            _sbCompiled     = root.Q<Label>("sb-compiled");
            _sbEntries      = root.Q<Label>("sb-entries");
            _sbFlatKeys     = root.Q<Label>("sb-flat-keys");
            _sbTables       = root.Q<Label>("sb-tables");
            _sbPath         = root.Q<Label>("sb-path");

            _tabExplorer  = root.Q("tab-explorer");
            _tabEditor    = root.Q("tab-editor");
            _tabTemplates = root.Q("tab-templates");

            _tabExplorer?.RegisterCallback<ClickEvent>(_ => ShowTab(0));
            _tabEditor?.RegisterCallback<ClickEvent>(_ => ShowTab(1));
            _tabTemplates?.RegisterCallback<ClickEvent>(_ => ShowTab(2));

            root.Q<Button>("btn-save")?.RegisterCallback<ClickEvent>(_ => SaveSource());
            root.Q<Button>("btn-compile")?.RegisterCallback<ClickEvent>(_ => Compile());
            _btnFoldAll = root.Q<Button>("btn-fold-all");
            _btnFoldAll?.RegisterCallback<ClickEvent>(_ => ToggleFoldAll());

            _codeField = new TextField
            {
                multiline = true,
                isDelayed = false,
                style     =
                {
                    flexGrow                = new StyleFloat(1f),
                    backgroundColor         = new StyleColor(new Color(0.031f, 0.047f, 0.078f)),
                    color                   = new StyleColor(new Color(0.91f, 0.93f, 0.96f)),
                    unityFontStyleAndWeight = new StyleEnum<FontStyle>(FontStyle.Normal),
                    fontSize                = new StyleLength(12),
                    whiteSpace              = new StyleEnum<WhiteSpace>(WhiteSpace.NoWrap),
                    paddingTop              = new StyleLength(12),
                    paddingLeft             = new StyleLength(12),
                },
            };

            _codeField.RegisterValueChangedCallback(evt =>
            {
                _sourceText = evt.newValue;
                _isDirty    = true;
                UpdateStatusBar(parsed: false, entryCount: 0, flatCount: 0, tableCount: 0);

                NotifyLspTextChanged(evt.newValue, evt.previousValue);
            });

            // OnEnable can run LoadAsset() before CreateGUI() builds this field, which would
            // leave it blank while _sourceText holds the file — and the next keystroke would
            // then overwrite _sourceText with just what was typed.
            if (!string.IsNullOrEmpty(_sourceText))
                _codeField.SetValueWithoutNotify(_sourceText);

            _panelEditor?.Add(_codeField);

            InitializeLspIntegration();
        }

        // ── Tab management ────────────────────────────────────────────────────

        private void ShowTab(int index)
        {
            _activeTab = index;

            SetTabActive(_tabExplorer,  index == 0);
            SetTabActive(_tabEditor,    index == 1);
            SetTabActive(_tabTemplates, index == 2);

            SetPanelVisible(_panelExplorer,  index == 0);
            SetPanelVisible(_panelEditor,    index == 1);
            SetPanelVisible(_panelTemplates, index == 2);

            if (index == 0 && !string.IsNullOrEmpty(_sourceText))
                RebuildExplorer();

            RefreshFoldAllButton();
        }

        private static void SetTabActive(VisualElement? tab, bool active)
        {
            if (tab == null) return;
            if (active) tab.AddToClassList("mdix-tab--active");
            else        tab.RemoveFromClassList("mdix-tab--active");
        }

        private static void SetPanelVisible(VisualElement? panel, bool visible)
        {
            if (panel == null) return;
            panel.style.display = visible
                ? new StyleEnum<DisplayStyle>(DisplayStyle.Flex)
                : new StyleEnum<DisplayStyle>(DisplayStyle.None);
        }

        // ── Asset loading ─────────────────────────────────────────────────────

        private void LoadAsset(MdixAsset asset)
        {
            // A different file has different sections; don't carry the old fold state over.
            if (!string.Equals(_currentPath, asset.ProjectRelativePath, StringComparison.Ordinal))
                _foldedSections.Clear();

            _currentAsset = asset;
            _currentPath  = asset.ProjectRelativePath;
            _sourceText   = asset.RawSource;
            _isDirty      = false;

            if (_fileLabel != null)
                _fileLabel.text = Path.GetFileName(_currentPath);

            // Without notify: loading a file is not an edit (it used to flag the file dirty).
            if (_codeField != null)
                _codeField.SetValueWithoutNotify(_sourceText);

            EditorPrefs.SetString(PrefKeyLastPath, _currentPath);

            Compile();
            NotifyLspDocumentOpened();
        }

        // ── Compile / parse ───────────────────────────────────────────────────

        private void Compile()
        {
            if (string.IsNullOrEmpty(_sourceText)) return;

            var result = Dix.LoadStr(_sourceText);

            if (result.IsFailure)
            {
                SetStatus($"✗  {result.Error.Message}", error: true);
                UpdateStatusBar(false, 0, 0, 0);
                return;
            }

            using var db = result.SuccessResult;

            var entryCount = db.EntryCount;
            var allKeys    = db.GetKeys().UnwrapOr(Array.Empty<string>());
            var flatCount  = allKeys.Count(k => !k.Contains('.'));
            var tableCount = allKeys.Count(k =>  k.Contains('.'));

            SetStatus($"✓  0 errors", error: false);
            UpdateStatusBar(true, entryCount, flatCount, tableCount);

            if (_activeTab == 0)
                RebuildExplorer(db);

            // Compiling only parses the text; it does not write it anywhere. It used to clear the
            // dirty flag here, so edits that were merely compiled were dropped silently on close.
        }

        // ── Explorer ──────────────────────────────────────────────────────────

        private void RebuildExplorer()
        {
            if (string.IsNullOrEmpty(_sourceText)) return;

            var result = Dix.LoadStr(_sourceText);
            if (result.IsFailure) return;

            using var db = result.SuccessResult;
            RebuildExplorer(db);
        }

        private void RebuildExplorer(MdixDatabase db)
        {
            if (_panelExplorer == null) return;

            _panelExplorer.Clear();
            _sections.Clear();

            var scroll = new ScrollView(ScrollViewMode.Vertical) { style = { flexGrow = 1 } };

            var allKeys    = db.GetKeys().UnwrapOr(Array.Empty<string>());
            var flatKeys   = allKeys.Where(k => !k.Contains('.')).ToArray();
            var groupKeys  = allKeys
                .Where(k => k.Contains('.'))
                .Select(k => k.Substring(0, k.IndexOf('.')))
                .Distinct()
                .ToArray();

            if (flatKeys.Length > 0)
            {
                var flatBody = new VisualElement();
                foreach (var key in flatKeys)
                    flatBody.Add(MakeKeyValueRow(db, key));

                AddSection(scroll, "FLAT PROPERTIES", flatBody);
            }

            foreach (var groupKey in groupKeys)
            {
                var valueType = db.GetValueType(groupKey);

                if (valueType == MdixValueType.Array)
                {
                    AddSection(scroll, $"ARRAY  —  {groupKey}", MakeArrayTable(db, groupKey));
                }
                else if (valueType == MdixValueType.Object)
                {
                    var tableBody = new VisualElement();
                    var childKeys = db.GetKeys(groupKey).UnwrapOr(Array.Empty<string>());
                    foreach (var child in childKeys)
                        tableBody.Add(MakeKeyValueRow(db, $"{groupKey}.{child}", labelOverride: child));

                    AddSection(scroll, $"TABLE  —  {groupKey}", tableBody);
                }
            }

            _panelExplorer.Add(scroll);

            RefreshFoldAllButton();
        }

        // ── Explorer folding ──────────────────────────────────────────────────

        private void AddSection(VisualElement parent, string title, VisualElement body)
        {
            var header  = MakeSectionHeader(title);
            var section = new ExplorerSection(title, header, body);
            _sections.Add(section);

            // The label that titles the section is also its fold handle.
            header.RegisterCallback<ClickEvent>(_ =>
            {
                ToggleSection(section);
                RefreshFoldAllButton();
            });

            ApplyFold(section, _foldedSections.Contains(title));

            parent.Add(header);
            parent.Add(body);
        }

        private static void ApplyFold(ExplorerSection section, bool folded)
        {
            section.Body.style.display = new StyleEnum<DisplayStyle>(
                folded ? DisplayStyle.None : DisplayStyle.Flex);

            section.Header.text = (folded ? "\u25B8  " : "\u25BE  ") + section.Title;
        }

        private void ToggleSection(ExplorerSection section)
        {
            // HashSet.Add is false when the title was already there, i.e. the section was folded.
            var folded = _foldedSections.Add(section.Title);
            if (!folded)
                _foldedSections.Remove(section.Title);

            ApplyFold(section, folded);
        }

        private void ToggleFoldAll()
        {
            if (_activeTab != 0 || _sections.Count == 0) return;

            // Anything still open => fold everything; everything already folded => open everything.
            var foldAll = _sections.Exists(s => !_foldedSections.Contains(s.Title));

            foreach (var section in _sections)
            {
                if (foldAll) _foldedSections.Add(section.Title);
                else         _foldedSections.Remove(section.Title);

                ApplyFold(section, foldAll);
            }

            RefreshFoldAllButton();
        }

        private void RefreshFoldAllButton()
        {
            if (_btnFoldAll == null) return;

            // Folding only means something in the Explorer; the Editor tab is one plain text field.
            _btnFoldAll.style.display = new StyleEnum<DisplayStyle>(
                _activeTab == 0 ? DisplayStyle.Flex : DisplayStyle.None);

            var allFolded = _sections.Count > 0 &&
                            !_sections.Exists(s => !_foldedSections.Contains(s.Title));

            _btnFoldAll.text = allFolded ? "Unfold All" : "Fold All";
            _btnFoldAll.SetEnabled(_sections.Count > 0);
        }

        private static Label MakeSectionHeader(string text)
        {
            return new Label(text)
            {
                style =
                {
                    backgroundColor         = new StyleColor(new Color(0.051f, 0.071f, 0.125f)),
                    color                   = new StyleColor(new Color(0.478f, 0.596f, 0.769f)),
                    fontSize                = new StyleLength(10),
                    unityFontStyleAndWeight = new StyleEnum<FontStyle>(FontStyle.Bold),
                    paddingTop              = new StyleLength(4),
                    paddingBottom           = new StyleLength(4),
                    paddingLeft             = new StyleLength(12),
                    borderBottomWidth       = new StyleFloat(1f),
                    borderBottomColor       = new StyleColor(new Color(0.102f, 0.149f, 0.251f)),
                }
            };
        }

        private static VisualElement MakeKeyValueRow(
            MdixDatabase db,
            string       fullPath,
            string?      labelOverride = null)
        {
            var row = new VisualElement();
            row.AddToClassList("mdix-kv-row");

            var valueType  = db.GetValueType(fullPath);
            var displayKey = labelOverride ?? fullPath;

            var keyLabel = new Label(displayKey);
            keyLabel.AddToClassList("mdix-kv__key");

            var valueStr   = GetValueDisplayString(db, fullPath, valueType);
            var valueLabel = new Label(valueStr);
            valueLabel.AddToClassList("mdix-kv__value");

            var typeLabel = new Label(valueType.ToString().ToLower());
            typeLabel.AddToClassList("mdix-kv__type");

            // Apply type-specific styling classes.
            switch (valueType)
            {
                case MdixValueType.Enum:
                    typeLabel.AddToClassList("mdix-kv__type--enum");
                    break;
                case MdixValueType.Bool:
                    typeLabel.AddToClassList("mdix-kv__type--bool");
                    break;
                case MdixValueType.String:
                case MdixValueType.Date:
                case MdixValueType.Timestamp:
                    typeLabel.AddToClassList("mdix-kv__type--string");
                    break;
                // Int, Long, Float, Double use the default blue mdix-kv__type style.
            }

            row.Add(keyLabel);
            row.Add(valueLabel);
            row.Add(typeLabel);

            return row;
        }

        private static VisualElement MakeArrayTable(MdixDatabase db, string arrayPath)
        {
            var container = new VisualElement();
            container.AddToClassList("mdix-table");

            var length = db.GetArrayLength(arrayPath).UnwrapOr(0);
            if (length == 0) return container;

            var firstItemPath = $"{arrayPath}[0]";
            var firstType     = db.GetValueType(firstItemPath);

            if (firstType == MdixValueType.Object)
            {
                var columns = db.GetKeys(firstItemPath).UnwrapOr(Array.Empty<string>());
                if (columns.Length == 0) return container;

                // Header row
                var headerRow = new VisualElement();
                headerRow.AddToClassList("mdix-table__header-row");

                var indexHeader = new Label("#");
                indexHeader.AddToClassList("mdix-table__header-cell");
                indexHeader.style.maxWidth = 40;
                headerRow.Add(indexHeader);

                foreach (var col in columns)
                {
                    var cell = new Label(col.ToUpper());
                    cell.AddToClassList("mdix-table__header-cell");
                    headerRow.Add(cell);
                }
                container.Add(headerRow);

                // Data rows
                for (int i = 0; i < length; i++)
                {
                    var itemPath  = $"{arrayPath}[{i}]";
                    var isBossRow = false;

                    foreach (var col in columns)
                    {
                        var colPath = $"{itemPath}.{col}";
                        if (db.GetValueType(colPath) == MdixValueType.Enum)
                        {
                            var field = db.GetEnumField(colPath).UnwrapOr(string.Empty);
                            if (field.Equals("BOSS", StringComparison.OrdinalIgnoreCase))
                            {
                                isBossRow = true;
                                break;
                            }
                        }
                    }

                    var row = new VisualElement();
                    row.AddToClassList("mdix-table__row");
                    if (isBossRow)
                        row.AddToClassList("mdix-table__row--boss");

                    var indexCell = new Label(i.ToString());
                    indexCell.AddToClassList("mdix-table__cell");
                    indexCell.style.maxWidth = 40;
                    indexCell.style.color    = new StyleColor(new Color(0.478f, 0.596f, 0.769f));
                    row.Add(indexCell);

                    foreach (var col in columns)
                    {
                        var colPath  = $"{itemPath}.{col}";
                        var colType  = db.GetValueType(colPath);
                        var valueStr = GetValueDisplayString(db, colPath, colType);

                        var cell = new Label(valueStr);
                        cell.AddToClassList("mdix-table__cell");

                        if (colType == MdixValueType.Enum)
                        {
                            var field = db.GetEnumField(colPath).UnwrapOr(string.Empty);
                            if (field.Equals("BOSS", StringComparison.OrdinalIgnoreCase))
                                cell.AddToClassList("mdix-table__cell--enum-boss");
                        }

                        row.Add(cell);
                    }

                    container.Add(row);
                }
            }
            else
            {
                // Scalar array — single column.
                var headerRow = new VisualElement();
                headerRow.AddToClassList("mdix-table__header-row");
                var hIndex = new Label("#");
                hIndex.AddToClassList("mdix-table__header-cell");
                hIndex.style.maxWidth = 40;
                var hValue = new Label("VALUE");
                hValue.AddToClassList("mdix-table__header-cell");
                headerRow.Add(hIndex);
                headerRow.Add(hValue);
                container.Add(headerRow);

                for (int i = 0; i < length; i++)
                {
                    var itemPath = $"{arrayPath}[{i}]";
                    var valType  = db.GetValueType(itemPath);
                    var valStr   = GetValueDisplayString(db, itemPath, valType);

                    var row = new VisualElement();
                    row.AddToClassList("mdix-table__row");

                    var iCell = new Label(i.ToString());
                    iCell.AddToClassList("mdix-table__cell");
                    iCell.style.maxWidth = 40;
                    iCell.style.color    = new StyleColor(new Color(0.478f, 0.596f, 0.769f));

                    var vCell = new Label(valStr);
                    vCell.AddToClassList("mdix-table__cell");

                    row.Add(iCell);
                    row.Add(vCell);
                    container.Add(row);
                }
            }

            return container;
        }

        private static string GetValueDisplayString(
            MdixDatabase  db,
            string        path,
            MdixValueType valueType)
        {
            return valueType switch
            {
                MdixValueType.String    => db.GetString(path).UnwrapOr(string.Empty),
                MdixValueType.Int       => db.GetInt(path).UnwrapOr(0).ToString(),
                // Long values displayed with L suffix to make the type obvious in the Explorer.
                MdixValueType.Long      => db.GetLong(path).UnwrapOr(0L).ToString() + "L",
                MdixValueType.Float     => db.GetFloat(path).UnwrapOr(0f)
                                              .ToString("G", System.Globalization.CultureInfo.InvariantCulture) + "f",
                MdixValueType.Double    => db.GetDouble(path).UnwrapOr(0d)
                                              .ToString("G", System.Globalization.CultureInfo.InvariantCulture),
                MdixValueType.Bool      => db.GetBool(path).UnwrapOr(false).ToString().ToLower(),
                MdixValueType.Enum      =>
                    $"{db.GetEnumName(path).UnwrapOr("?")}." +
                    $"{db.GetEnumField(path).UnwrapOr("?")}",
                MdixValueType.HexColor  => db.GetString(path).UnwrapOr("#?"),
                MdixValueType.Date      => db.GetString(path).UnwrapOr("?"),
                MdixValueType.Timestamp => db.GetString(path).UnwrapOr("?"),
                MdixValueType.Null      => "null",
                MdixValueType.Array     => "[array]",
                MdixValueType.Object    => "{object}",
                MdixValueType.Tuple     => "(tuple)",
                _                       => "?",
            };
        }

        // ── Templates panel ───────────────────────────────────────────────────

        private void BuildTemplatesPanel()
        {
            if (_panelTemplates == null) return;

            var scroll = new ScrollView(ScrollViewMode.Vertical) { style = { flexGrow = 1 } };

            var header = new Label("Choose a template to create a new .mdix file")
            {
                style =
                {
                    color         = new StyleColor(new Color(0.478f, 0.596f, 0.769f)),
                    fontSize      = new StyleLength(12),
                    paddingTop    = new StyleLength(16),
                    paddingLeft   = new StyleLength(16),
                    paddingBottom = new StyleLength(8),
                }
            };
            scroll.Add(header);

            var grid = new VisualElement();
            grid.AddToClassList("mdix-template-grid");

            var templateDefs = new (string title, string desc, string content)[]
            {
                ("Blank",            "Empty file with @CONFIG stub",                          Templates.Blank),
                ("Game Enemies",     "Enemy array with AI enum and @QUICKFUNCS formula",     Templates.GameEnemies),
                ("Inventory Items",  "Item array with type and rarity enums",                Templates.InventoryItems),
                ("App Config",       "Environment enum, log level, server block",            Templates.AppConfig),
                ("Server Config",    "Multi-environment with @QUICKFUNCS",                   Templates.MultiEnvServer),
                ("Encrypted Secrets","API keys with @DLM encryption",                        Templates.EncryptedSecrets),
                ("Player Save",      "Save game data with position and flags",               Templates.PlayerSave),
            };

            foreach (var (title, desc, content) in templateDefs)
            {
                var card = new VisualElement();
                card.AddToClassList("mdix-template-card");

                var cardTitle = new Label(title);
                cardTitle.AddToClassList("mdix-template-card__title");

                var cardDesc = new Label(desc);
                cardDesc.AddToClassList("mdix-template-card__desc");

                card.Add(cardTitle);
                card.Add(cardDesc);

                var capturedContent = content;
                var capturedTitle   = title;

                card.RegisterCallback<ClickEvent>(_ =>
                    CreateFromTemplate(capturedTitle, capturedContent));

                grid.Add(card);
            }

            scroll.Add(grid);
            _panelTemplates.Add(scroll);
        }

        private static void CreateFromTemplate(string title, string content)
        {
            var defaultName = title.ToLower().Replace(" ", "_");
            var folder      = AssetDatabase.GetAssetPath(Selection.activeObject);

            if (string.IsNullOrEmpty(folder) || !Directory.Exists(folder))
                folder = "Assets";

            var path = AssetDatabase.GenerateUniqueAssetPath(
                $"{folder}/{defaultName}.mdix");

            File.WriteAllText(path, content);
            AssetDatabase.Refresh();

            var asset = AssetDatabase.LoadAssetAtPath<MdixAsset>(path);
            if (asset != null)
                Selection.activeObject = asset;
        }

        // ── Save ──────────────────────────────────────────────────────────────

        private void SaveSource()
        {
            // A scratch document (typed straight into the Editor tab) has no file yet: Save As.
            if (string.IsNullOrEmpty(_currentPath))
            {
                var newPath = PromptAndWriteNewFile();
                if (newPath != null)
                    AdoptSavedPath(newPath);

                return;
            }

            try
            {
                File.WriteAllText(_currentPath, _sourceText);
                AssetDatabase.ImportAsset(_currentPath);
                _isDirty = false;
                SetStatus("✓  Saved", error: false);
            }
            catch (Exception ex)
            {
                SetStatus($"✗  Save failed: {ex.Message}", error: true);
            }
        }

        /// <summary>
        /// Asks where an unsaved (scratch) document should live and writes it there. Returns the
        /// project-relative path, or null if the person cancelled or the write failed. Leaves the
        /// window's own state alone, so it is also safe to call while the window is closing.
        /// </summary>
        private string? PromptAndWriteNewFile()
        {
            var path = EditorUtility.SaveFilePanelInProject(
                "Save MDIX file", "untitled", "mdix",
                "Choose where to save this .mdix file inside the project.");

            if (string.IsNullOrEmpty(path)) return null; // cancelled

            try
            {
                File.WriteAllText(path, _sourceText);
                AssetDatabase.ImportAsset(path);
                return path;
            }
            catch (Exception ex)
            {
                SetStatus($"✗  Save failed: {ex.Message}", error: true);
                return null;
            }
        }

        /// <summary>Points the window at the file a scratch document was just saved as.</summary>
        private void AdoptSavedPath(string path)
        {
            _currentAsset = AssetDatabase.LoadAssetAtPath<MdixAsset>(path);
            _currentPath  = path;
            _isDirty      = false;

            if (_fileLabel != null)
                _fileLabel.text = Path.GetFileName(path);

            EditorPrefs.SetString(PrefKeyLastPath, path);

            Compile();                  // refreshes the status bar (file name included) and the Explorer
            NotifyLspDocumentOpened();  // the server drops the untitled document and opens this file
            SetStatus("✓  Saved", error: false);
        }

        // ── Status helpers ────────────────────────────────────────────────────

        private void SetStatus(string text, bool error)
        {
            if (_statusLabel == null) return;
            _statusLabel.text = text;
            _statusLabel.EnableInClassList("mdix-status--error", error);
        }

        private void UpdateStatusBar(
            bool parsed, int entryCount, int flatCount, int tableCount)
        {
            if (_sbCompiled != null)
            {
                _sbCompiled.text = parsed ? "● Compiled" : "● Not compiled";
                _sbCompiled.EnableInClassList("mdix-statusbar__item--ok",    parsed);
                _sbCompiled.EnableInClassList("mdix-statusbar__item--error", !parsed);
            }

            if (_sbEntries  != null) _sbEntries.text  = $"{entryCount} entries";
            if (_sbFlatKeys != null) _sbFlatKeys.text = $"{flatCount} flat keys";
            if (_sbTables   != null) _sbTables.text   = $"{tableCount} table keys";
            if (_sbPath     != null) _sbPath.text     = Path.GetFileName(_currentPath);
        }

        // ── OnEnable / OnDisable ──────────────────────────────────────────────

        private void OnEnable()
        {
            if (_currentAsset == null)
            {
                var lastPath = EditorPrefs.GetString(PrefKeyLastPath, string.Empty);
                if (!string.IsNullOrEmpty(lastPath))
                {
                    var asset = AssetDatabase.LoadAssetAtPath<MdixAsset>(lastPath);
                    if (asset != null)
                        LoadAsset(asset);
                }
            }
        }

        private void OnDisable()
        {
            if (_isDirty)
            {
                var scratch = string.IsNullOrEmpty(_currentPath);
                var name    = scratch ? "Untitled" : Path.GetFileName(_currentPath);

                if (EditorUtility.DisplayDialog(
                    "Unsaved Changes",
                    $"'{name}' has unsaved changes. Save before closing?",
                    "Save", "Discard"))
                {
                    if (scratch)
                    {
                        // Only write the file: the window is going away, so don't rebind its UI.
                        var saved = PromptAndWriteNewFile();
                        if (saved != null)
                            EditorPrefs.SetString(PrefKeyLastPath, saved);
                    }
                    else
                    {
                        SaveSource();
                    }
                }
            }

            ShutdownLspIntegration();
        }
    }
}
