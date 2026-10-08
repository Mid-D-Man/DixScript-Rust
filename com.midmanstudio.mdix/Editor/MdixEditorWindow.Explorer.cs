using System;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.UIElements;
using MidManStudio.Mdix.Core;

namespace MidManStudio.Mdix.Unity.Editor
{
    /// <summary>
    /// The Explorer tab: the compiled data as a tree, to any depth.
    ///
    /// Shapes it handles, by what the engine reports for each path:
    ///   - scalar               a key / value / type row
    ///   - object               a foldable group of its members
    ///   - array of flat objects   a table, one row per item (the compact view for typical data)
    ///   - array of scalars     one row per item
    ///   - anything else        one foldable node per item, which recurses
    ///
    /// The engine lists array items again as extra keys ("enemies[0]") next to the array itself, and
    /// returns keys in no fixed order, so names are reduced to their first segment and sorted here.
    /// Large data is capped (items per array, rows overall, depth) and says so when it is.
    /// </summary>
    public sealed partial class MdixEditorWindow
    {
        private const int MaxExplorerDepth   = 12;
        private const int MaxArrayItemsShown = 200;
        private const int MaxExplorerRows    = 4000;
        private const int AutoFoldAboveItems = 8;      // item nodes of a longer mixed array start folded

        private static readonly string[] LeadingNames = { "id", "name", "title", "label" };

        // The keys of folded nodes are remembered across rebuilds, so the layout survives every
        // recompile. A key seen for the first time gets its default; after that the person's choice rules.
        private readonly HashSet<string>       _foldedSections = new HashSet<string>();
        private readonly HashSet<string>       _knownFoldKeys  = new HashSet<string>();
        private readonly List<ExplorerSection> _sections       = new List<ExplorerSection>();

        private int _explorerRowBudget;

        private readonly struct ExplorerSection
        {
            public readonly string        Key;
            public readonly string        Title;
            public readonly Label         Header;
            public readonly VisualElement Body;

            public ExplorerSection(string key, string title, Label header, VisualElement body)
            {
                Key    = key;
                Title  = title;
                Header = header;
                Body   = body;
            }
        }

        private enum ItemShape { Scalars, FlatObjects, Mixed }

        // ── Build ─────────────────────────────────────────────────────────────

        private void BuildExplorer(MdixDatabase db)
        {
            if (_panelExplorer == null) return;

            _panelExplorer.Clear();
            _sections.Clear();
            _explorerRowBudget = MaxExplorerRows;

            var scroll = new ScrollView(ScrollViewMode.Vertical) { style = { flexGrow = 1 } };

            var flat       = new List<string>();
            var containers = new List<string>();

            foreach (var name in TopLevelNames(db))
            {
                var type = db.GetValueType(name);
                if (type == MdixValueType.Array || type == MdixValueType.Object) containers.Add(name);
                else if (type != MdixValueType.Unknown)                          flat.Add(name);
            }

            if (flat.Count > 0)
            {
                var flatBody = new VisualElement();
                foreach (var name in flat)
                {
                    if (!TakeExplorerRow(flatBody)) break;
                    flatBody.Add(MakeKeyValueRow(db, name));
                }

                AddSection(scroll, "#flat", "FLAT PROPERTIES", flatBody, defaultFolded: false, nested: false);
            }

            foreach (var name in containers)
            {
                var type  = db.GetValueType(name);
                var body  = type == MdixValueType.Array
                    ? BuildArrayBody(db, name, 1)
                    : BuildObjectBody(db, name, 1);

                var title = (type == MdixValueType.Array ? "ARRAY  —  " : "TABLE  —  ") +
                            name + "  " + CountBadge(db, name, type);

                AddSection(scroll, "/" + name, title, body, defaultFolded: false, nested: false);
            }

            _panelExplorer.Add(scroll);

            RefreshFoldAllButton();
        }

        private VisualElement BuildObjectBody(MdixDatabase db, string path, int depth)
        {
            var body       = new VisualElement();
            var containers = new List<KeyValuePair<string, MdixValueType>>();

            foreach (var child in ChildNames(db, path))
            {
                var childPath = path + "." + child;
                var type      = db.GetValueType(childPath);

                if (type == MdixValueType.Array || type == MdixValueType.Object)
                {
                    containers.Add(new KeyValuePair<string, MdixValueType>(child, type));
                }
                else if (type != MdixValueType.Unknown)
                {
                    if (!TakeExplorerRow(body)) return body;
                    body.Add(MakeKeyValueRow(db, childPath, labelOverride: child));
                }
            }

            // Plain values first, nested groups after them.
            foreach (var entry in containers)
            {
                if (!TakeExplorerRow(body)) return body;
                body.Add(MakeFoldNode(db, path + "." + entry.Key, entry.Key, entry.Value, depth, false));
            }

            return body;
        }

        private VisualElement BuildArrayBody(MdixDatabase db, string path, int depth)
        {
            var body   = new VisualElement();
            var length = db.GetArrayLength(path).UnwrapOr(0);

            if (length == 0)
            {
                body.Add(MakeNoteLabel("(empty)"));
                return body;
            }

            var shown = Math.Min(length, MaxArrayItemsShown);

            switch (ClassifyItems(db, path, shown))
            {
                case ItemShape.FlatObjects:
                    body.Add(MakeFlatTable(db, path, shown));
                    break;

                case ItemShape.Scalars:
                    for (int i = 0; i < shown; i++)
                    {
                        if (!TakeExplorerRow(body)) break;
                        body.Add(MakeKeyValueRow(db, path + "[" + i + "]", labelOverride: "[" + i + "]"));
                    }
                    break;

                default:
                    for (int i = 0; i < shown; i++)
                    {
                        if (!TakeExplorerRow(body)) break;

                        var itemPath = path + "[" + i + "]";
                        var type     = db.GetValueType(itemPath);

                        if (type == MdixValueType.Array || type == MdixValueType.Object)
                        {
                            var label = "[" + i + "]" + ItemHint(db, itemPath, type);
                            body.Add(MakeFoldNode(db, itemPath, label, type, depth, length > AutoFoldAboveItems));
                        }
                        else if (type != MdixValueType.Unknown)
                        {
                            body.Add(MakeKeyValueRow(db, itemPath, labelOverride: "[" + i + "]"));
                        }
                    }
                    break;
            }

            if (length > shown)
                body.Add(MakeNoteLabel("…  " + (length - shown) + " more items are not shown. The Editor tab has all of them."));

            return body;
        }

        /// <summary>A foldable node for a nested object or array, indented under its parent.</summary>
        private VisualElement MakeFoldNode(
            MdixDatabase db, string path, string label, MdixValueType type, int depth, bool defaultFolded)
        {
            if (depth >= MaxExplorerDepth)
                return MakeNoteLabel(label + "  …  nested too deeply to show here");

            var body = type == MdixValueType.Object
                ? BuildObjectBody(db, path, depth + 1)
                : BuildArrayBody(db, path, depth + 1);

            var node = new VisualElement();
            node.style.marginLeft = new StyleLength(14);

            AddSection(node, path, label + "  " + CountBadge(db, path, type), body, defaultFolded, nested: true);
            return node;
        }

        private static string CountBadge(MdixDatabase db, string path, MdixValueType type)
        {
            if (type == MdixValueType.Array)
                return "[" + db.GetArrayLength(path).UnwrapOr(0) + "]";

            return "{" + ChildNames(db, path).Count + "}";
        }

        // ── Shapes ────────────────────────────────────────────────────────────

        private static ItemShape ClassifyItems(MdixDatabase db, string path, int shown)
        {
            var allScalar = true;
            var allFlat   = true;

            for (int i = 0; i < shown && (allScalar || allFlat); i++)
            {
                var itemPath = path + "[" + i + "]";
                var type     = db.GetValueType(itemPath);

                if (type == MdixValueType.Unknown)
                {
                    allScalar = false;
                    allFlat   = false;
                    break;
                }

                if (type == MdixValueType.Array || type == MdixValueType.Object) allScalar = false;

                if (type != MdixValueType.Object || !IsFlatObject(db, itemPath)) allFlat = false;
            }

            return allScalar ? ItemShape.Scalars : (allFlat ? ItemShape.FlatObjects : ItemShape.Mixed);
        }

        /// <summary>An object whose members are all plain values, so it fits on one table row.</summary>
        private static bool IsFlatObject(MdixDatabase db, string path)
        {
            var children = ChildNames(db, path);
            if (children.Count == 0) return false;

            foreach (var child in children)
            {
                var type = db.GetValueType(path + "." + child);
                if (type == MdixValueType.Array || type == MdixValueType.Object || type == MdixValueType.Unknown)
                    return false;
            }
            return true;
        }

        private VisualElement MakeFlatTable(MdixDatabase db, string arrayPath, int shown)
        {
            var container = new VisualElement();
            container.AddToClassList("mdix-table");

            // Columns: every name that appears in any shown item.
            var columns = new List<string>();
            for (int i = 0; i < shown; i++)
                foreach (var name in ChildNames(db, arrayPath + "[" + i + "]"))
                    if (!columns.Contains(name)) columns.Add(name);

            columns.Sort(CompareNames);
            if (columns.Count == 0) return container;

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

            for (int i = 0; i < shown; i++)
            {
                if (!TakeExplorerRow(container)) break;

                var itemPath  = arrayPath + "[" + i + "]";
                var isBossRow = false;

                foreach (var col in columns)
                {
                    var colPath = itemPath + "." + col;
                    if (db.GetValueType(colPath) == MdixValueType.Enum &&
                        db.GetEnumField(colPath).UnwrapOr(string.Empty).Equals("BOSS", StringComparison.OrdinalIgnoreCase))
                    {
                        isBossRow = true;
                        break;
                    }
                }

                var row = new VisualElement();
                row.AddToClassList("mdix-table__row");
                if (isBossRow) row.AddToClassList("mdix-table__row--boss");

                var indexCell = new Label(i.ToString());
                indexCell.AddToClassList("mdix-table__cell");
                indexCell.style.maxWidth = 40;
                indexCell.style.color    = new StyleColor(new Color(0.478f, 0.596f, 0.769f));
                row.Add(indexCell);

                foreach (var col in columns)
                {
                    var colPath = itemPath + "." + col;
                    var colType = db.GetValueType(colPath);

                    // An item that lacks this column leaves the cell empty.
                    var cell = new Label(colType == MdixValueType.Unknown
                        ? string.Empty
                        : GetValueDisplayString(db, colPath, colType));
                    cell.AddToClassList("mdix-table__cell");

                    if (colType == MdixValueType.Enum &&
                        db.GetEnumField(colPath).UnwrapOr(string.Empty).Equals("BOSS", StringComparison.OrdinalIgnoreCase))
                        cell.AddToClassList("mdix-table__cell--enum-boss");

                    row.Add(cell);
                }

                container.Add(row);
            }

            return container;
        }

        /// <summary>A short text for an item's header: the value of its id, name, title or label if it has one.</summary>
        private static string ItemHint(MdixDatabase db, string itemPath, MdixValueType type)
        {
            if (type != MdixValueType.Object) return string.Empty;

            foreach (var key in LeadingNames)
            {
                var path = itemPath + "." + key;
                var kind = db.GetValueType(path);
                if (kind == MdixValueType.Unknown || kind == MdixValueType.Array || kind == MdixValueType.Object)
                    continue;

                var text = GetValueDisplayString(db, path, kind);
                if (text.Length > 40) text = text.Substring(0, 40) + "…";
                return "  " + text;
            }
            return string.Empty;
        }

        // ── Names ─────────────────────────────────────────────────────────────

        /// <summary>
        /// The names directly under a path. The engine also lists array items as "name[0]" and returns
        /// keys in no fixed order, so each key is cut to its first segment and the result is sorted.
        /// </summary>
        private static List<string> ChildNames(MdixDatabase db, string path)
        {
            var keys = string.IsNullOrEmpty(path) ? db.GetKeys() : db.GetKeys(path);
            return NamesFrom(keys.UnwrapOr(Array.Empty<string>()));
        }

        private static List<string> TopLevelNames(MdixDatabase db) => ChildNames(db, string.Empty);

        private static List<string> NamesFrom(string[] keys)
        {
            var set = new HashSet<string>(StringComparer.Ordinal);
            foreach (var key in keys)
            {
                var cut  = key.IndexOfAny(new[] { '.', '[' });
                var name = cut < 0 ? key : key.Substring(0, cut);
                if (name.Length > 0) set.Add(name);
            }

            var list = new List<string>(set);
            list.Sort(CompareNames);
            return list;
        }

        /// <summary>id, name, title and label first, then alphabetical.</summary>
        private static int CompareNames(string a, string b)
        {
            var ra = Array.IndexOf(LeadingNames, a.ToLowerInvariant());
            var rb = Array.IndexOf(LeadingNames, b.ToLowerInvariant());
            if (ra < 0) ra = int.MaxValue;
            if (rb < 0) rb = int.MaxValue;

            return ra != rb ? ra.CompareTo(rb) : string.Compare(a, b, StringComparison.OrdinalIgnoreCase);
        }

        /// <summary>Counts for the status bar: plain top-level values, and top-level objects and arrays.</summary>
        private static void CountTopLevel(MdixDatabase db, out int flat, out int tables)
        {
            flat = 0; tables = 0;

            foreach (var name in TopLevelNames(db))
            {
                var type = db.GetValueType(name);
                if (type == MdixValueType.Array || type == MdixValueType.Object) tables++;
                else if (type != MdixValueType.Unknown)                          flat++;
            }
        }

        // ── Row budget and small elements ─────────────────────────────────────

        private bool TakeExplorerRow(VisualElement host)
        {
            if (_explorerRowBudget > 0)
            {
                _explorerRowBudget--;
                return true;
            }

            if (_explorerRowBudget == 0)
            {
                _explorerRowBudget = -1;          // say so once, then stay quiet
                host.Add(MakeNoteLabel("…  too many rows to show. The rest is hidden here; the Editor tab and Search see everything."));
            }
            return false;
        }

        private static Label MakeNoteLabel(string text)
        {
            return new Label(text)
            {
                style =
                {
                    color                   = new StyleColor(new Color(0.45f, 0.52f, 0.63f)),
                    fontSize                = new StyleLength(10),
                    unityFontStyleAndWeight = new StyleEnum<FontStyle>(FontStyle.Italic),
                    paddingTop              = new StyleLength(2),
                    paddingBottom           = new StyleLength(2),
                    paddingLeft             = new StyleLength(12),
                }
            };
        }

        private static Label MakeNodeHeader(string text)
        {
            return new Label(text)
            {
                style =
                {
                    color         = new StyleColor(new Color(0.62f, 0.72f, 0.88f)),
                    fontSize      = new StyleLength(11),
                    paddingTop    = new StyleLength(3),
                    paddingBottom = new StyleLength(3),
                    paddingLeft   = new StyleLength(8),
                }
            };
        }

        // ── Folding ───────────────────────────────────────────────────────────

        private void AddSection(
            VisualElement parent, string key, string title, VisualElement body, bool defaultFolded, bool nested)
        {
            var header  = nested ? MakeNodeHeader(title) : MakeSectionHeader(title);
            var section = new ExplorerSection(key, title, header, body);
            _sections.Add(section);

            // The label that titles the section is also its fold handle.
            header.RegisterCallback<ClickEvent>(_ =>
            {
                ToggleSection(section);
                RefreshFoldAllButton();
            });

            if (_knownFoldKeys.Add(key) && defaultFolded)
                _foldedSections.Add(key);

            ApplyFold(section, _foldedSections.Contains(key));

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
            // HashSet.Add is false when the key was already there, i.e. the section was folded.
            var folded = _foldedSections.Add(section.Key);
            if (!folded)
                _foldedSections.Remove(section.Key);

            ApplyFold(section, folded);
        }

        private void ToggleFoldAll()
        {
            if (_activeTab != 0 || _sections.Count == 0) return;

            // Anything still open => fold everything; everything already folded => open everything.
            var foldAll = _sections.Exists(s => !_foldedSections.Contains(s.Key));

            foreach (var section in _sections)
            {
                if (foldAll) _foldedSections.Add(section.Key);
                else         _foldedSections.Remove(section.Key);

                ApplyFold(section, foldAll);
            }

            RefreshFoldAllButton();
        }

        private void RefreshFoldAllButton()
        {
            if (_btnFoldAll == null) return;

            // Folding only means something in the Explorer; the other tabs have nothing to fold.
            _btnFoldAll.style.display = new StyleEnum<DisplayStyle>(
                _activeTab == 0 ? DisplayStyle.Flex : DisplayStyle.None);

            var allFolded = _sections.Count > 0 &&
                            !_sections.Exists(s => !_foldedSections.Contains(s.Key));

            _btnFoldAll.text = allFolded ? "Unfold All" : "Fold All";
            _btnFoldAll.SetEnabled(_sections.Count > 0);
        }
    }
}
