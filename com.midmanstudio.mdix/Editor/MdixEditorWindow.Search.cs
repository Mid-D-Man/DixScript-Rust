using System;
using System.Collections.Generic;
using System.IO;
using System.Text.RegularExpressions;
using UnityEditor;
using UnityEngine;
using UnityEngine.UIElements;
using MidManStudio.Mdix.Unity;

namespace MidManStudio.Mdix.Unity.Editor
{
    /// <summary>
    /// The Search tab: find text in every .mdix file of the project (Assets and packages).
    ///
    /// The search runs here, on the files themselves, so it needs nothing from the language server
    /// and sees files the server has never been told about. The document that is open in the window
    /// is searched as it is in the editor, unsaved edits included. The matching lives in
    /// <see cref="MdixProjectSearch"/>.
    ///
    /// Typing searches after a short pause, Enter searches at once. Clicking a result opens the file
    /// in the Editor tab with the match selected.
    ///
    /// Two modes, switched by the "File names" box (remembered between sessions): file names lists the
    /// .mdix files whose path matches — and every file when the box is empty, so the tab doubles as a
    /// file browser — and the other mode looks inside the files.
    /// </summary>
    public sealed partial class MdixEditorWindow
    {
        private const string UntitledKey = "(untitled)";
        private const int    SearchDelayMs = 300;
        private const int    MaxFileRows   = 500;
        private const string PrefKeySearchFiles = "MdixStudio_SearchFileNames";

        private bool SearchFilesMode => _searchFilesMode != null && _searchFilesMode.value;

        private TextField  _searchField;
        private Toggle     _searchMatchCase;
        private Toggle     _searchRegex;
        private Toggle     _searchNamesOnly;
        private Toggle     _searchFilesMode;
        private Label      _searchSummary;
        private ScrollView _searchResults;
        private IVisualElementScheduledItem _searchTimer;

        // ── Build ─────────────────────────────────────────────────────────────

        private void BuildSearchPanel()
        {
            if (_panelSearch == null) return;

            _panelSearch.Clear();

            var box = new VisualElement
            {
                style =
                {
                    flexGrow        = new StyleFloat(1f),
                    paddingTop      = new StyleLength(10),
                    paddingLeft     = new StyleLength(12),
                    paddingRight    = new StyleLength(12),
                    paddingBottom   = new StyleLength(6),
                },
            };

            _searchField = new TextField
            {
                multiline = false,
                isDelayed = false,
                label     = string.Empty,
                style =
                {
                    // Not flexGrow: in this column that made the query box swallow half the panel.
                    flexGrow        = new StyleFloat(0f),
                    flexShrink      = new StyleFloat(0f),
                    marginLeft      = new StyleLength(0f),
                    marginRight     = new StyleLength(0f),
                    marginBottom    = new StyleLength(4),
                },
            };

            _searchField.tooltip = "What to look for. Enter searches at once.";
            _searchField.RegisterValueChangedCallback(_ => ScheduleSearch());
            _searchField.RegisterCallback<KeyDownEvent>(evt =>
            {
                if (evt.keyCode != KeyCode.Return && evt.keyCode != KeyCode.KeypadEnter) return;

                RunSearch();
                evt.StopPropagation();
            });

            var options = new VisualElement
            {
                style = { flexDirection = new StyleEnum<FlexDirection>(FlexDirection.Row) },
            };

            _searchFilesMode = MakeSearchToggle("File names",  "On: list the .mdix files whose name or folder matches (every file when empty). Off: search inside the files.");
            _searchFilesMode.SetValueWithoutNotify(EditorPrefs.GetBool(PrefKeySearchFiles, true));
            _searchFilesMode.RegisterValueChangedCallback(evt =>
            {
                EditorPrefs.SetBool(PrefKeySearchFiles, evt.newValue);
                UpdateSearchModeUi();
            });

            _searchMatchCase = MakeSearchToggle("Match case",  "Treat upper and lower case as different letters.");
            _searchRegex     = MakeSearchToggle("Regex",       "Read the text as a regular expression.");
            _searchNamesOnly = MakeSearchToggle("Names only",  "Only count matches on keys, sections, enums and functions. Skips values and comments.");

            options.Add(_searchFilesMode);
            options.Add(_searchMatchCase);
            options.Add(_searchRegex);
            options.Add(_searchNamesOnly);

            _searchSummary = new Label(string.Empty)
            {
                style =
                {
                    color         = new StyleColor(new Color(0.478f, 0.596f, 0.769f)),
                    fontSize      = new StyleLength(11),
                    paddingTop    = new StyleLength(6),
                    paddingBottom = new StyleLength(6),
                },
            };

            _searchResults = new ScrollView(ScrollViewMode.Vertical) { style = { flexGrow = new StyleFloat(1f) } };

            box.Add(_searchField);
            box.Add(options);
            box.Add(_searchSummary);
            box.Add(_searchResults);

            _panelSearch.Add(box);

            UpdateSearchModeUi();
        }

        /// <summary>"Names only" and the idle hint only make sense for the mode that is on.</summary>
        private void UpdateSearchModeUi()
        {
            _searchNamesOnly?.SetEnabled(!SearchFilesMode);

            // File-name mode describes its own list ("12 files in the project"); the hint is only
            // for the mode that shows nothing until something is typed.
            if (!SearchFilesMode && _searchField != null && string.IsNullOrEmpty(_searchField.value))
                SetSearchSummary(IdleSearchHint(), error: false);
        }

        private string IdleSearchHint() => SearchFilesMode
            ? "Type part of a file name or folder, or leave this empty to list every .mdix file."
            : "Type to search inside every .mdix file in the project.";

        private Toggle MakeSearchToggle(string label, string tooltip)
        {
            var toggle = new Toggle(label)
            {
                value = false,
                style = { marginRight = new StyleLength(14) },
            };

            toggle.tooltip = tooltip;
            toggle.RegisterValueChangedCallback(_ => RunSearch());
            return toggle;
        }

        // ── Run ───────────────────────────────────────────────────────────────

        private void ScheduleSearch()
        {
            if (_panelSearch == null) return;

            if (_searchTimer != null) _searchTimer.Pause();

            _searchTimer = _panelSearch.schedule.Execute(RunSearch);
            _searchTimer.ExecuteLater(SearchDelayMs);
        }

        private void FocusSearchField()
        {
            if (_searchField == null) return;

            // The panel has only just been shown, so wait a moment for it to take part in layout.
            _searchField.schedule.Execute(() => _searchField.Focus()).ExecuteLater(30);
        }

        private void RunSearch()
        {
            if (_searchField == null || _searchResults == null) return;

            if (_searchTimer != null) _searchTimer.Pause();

            var query = _searchField.value ?? string.Empty;

            if (SearchFilesMode)
            {
                RunFileNameSearch(query);
                return;
            }

            if (query.Length == 0)
            {
                _searchResults.Clear();
                SetSearchSummary(IdleSearchHint(), error: false);
                return;
            }

            var options = new MdixSearchOptions
            {
                Query     = query,
                MatchCase = _searchMatchCase != null && _searchMatchCase.value,
                UseRegex  = _searchRegex     != null && _searchRegex.value,
                NamesOnly = _searchNamesOnly != null && _searchNamesOnly.value,
            };

            var result = MdixProjectSearch.Run(CollectSearchSources(), options);
            ShowSearchResults(result);
        }

        // ── File names ────────────────────────────────────────────────────────

        private void RunFileNameSearch(string query)
        {
            var matchCase = _searchMatchCase != null && _searchMatchCase.value;
            var useRegex  = _searchRegex     != null && _searchRegex.value;

            Regex pattern = null;
            if (query.Length > 0 && useRegex)
            {
                try
                {
                    pattern = new Regex(query, matchCase ? RegexOptions.None : RegexOptions.IgnoreCase);
                }
                catch (ArgumentException ex)
                {
                    _searchResults.Clear();
                    SetSearchSummary("✗  " + ex.Message, error: true);
                    return;
                }
            }

            var comparison = matchCase ? StringComparison.Ordinal : StringComparison.OrdinalIgnoreCase;

            // Paths only: nothing is read, so this stays instant however many files there are.
            var paths = new List<string>();
            var seen  = new HashSet<string>(StringComparer.OrdinalIgnoreCase);

            foreach (var assetPath in AssetDatabase.GetAllAssetPaths())
            {
                if (!assetPath.EndsWith(".mdix", StringComparison.OrdinalIgnoreCase)) continue;
                if (seen.Add(assetPath)) paths.Add(assetPath);
            }

            // A document that was never saved has no path but is still part of the work.
            if (string.IsNullOrEmpty(_currentPath) && !string.IsNullOrEmpty(_sourceText))
                paths.Add(UntitledKey);

            paths.Sort((a, b) => string.Compare(a, b, StringComparison.OrdinalIgnoreCase));

            _searchResults.Clear();

            var total = 0;
            var shown = 0;

            foreach (var path in paths)
            {
                var display = path == UntitledKey ? "Untitled (not saved yet)" : path;

                var matches = query.Length == 0 ||
                              (pattern != null ? pattern.IsMatch(display) : display.IndexOf(query, comparison) >= 0);
                if (!matches) continue;

                total++;
                if (shown >= MaxFileRows) continue;

                _searchResults.Add(MakeFileRow(path, display, query, pattern, comparison));
                shown++;
            }

            var summary = total == 0
                ? (query.Length == 0 ? "There are no .mdix files in the project yet." : "No file name matches.")
                : total + (total == 1 ? " file" : " files") + (query.Length == 0 ? " in the project" : " match");

            if (total > shown) summary += ". The list is cut here; type more of the name to see the rest";

            SetSearchSummary(summary, error: false);
        }

        private VisualElement MakeFileRow(string path, string display, string query, Regex pattern, StringComparison comparison)
        {
            var slash  = display.LastIndexOf('/');
            var folder = slash >= 0 ? display.Substring(0, slash) : string.Empty;
            var name   = slash >= 0 ? display.Substring(slash + 1) : display;

            // The matched part of the file name in gold; a match that lies in the folder is left plain.
            var start  = -1;
            var length = 0;

            if (query.Length > 0)
            {
                if (pattern != null)
                {
                    var match = pattern.Match(name);
                    if (match.Success && match.Length > 0) { start = match.Index; length = match.Length; }
                }
                else
                {
                    start = name.IndexOf(query, comparison);
                    length = query.Length;
                }
            }

            var nameRich = start < 0
                ? Highlight.MdixRichText.Escape(name)
                : Highlight.MdixRichText.Escape(name.Substring(0, start)) +
                  "<color=#FFD27F>" + Highlight.MdixRichText.Escape(name.Substring(start, length)) + "</color>" +
                  Highlight.MdixRichText.Escape(name.Substring(start + length));

            var isOpen = path == UntitledKey
                ? string.IsNullOrEmpty(_currentPath)
                : string.Equals(path, _currentPath, StringComparison.Ordinal);

            var rich = "<b>" + nameRich + "</b>" +
                       (folder.Length == 0 ? string.Empty : "   <color=#6B7A90>" + Highlight.MdixRichText.Escape(folder) + "</color>") +
                       (isOpen ? "   <color=#5C6B80>" + (_isDirty ? "open, unsaved changes" : "open") + "</color>" : string.Empty);

            var row = new Label(rich)
            {
                enableRichText = true,
                tooltip        = display,
                style =
                {
                    paddingLeft     = new StyleLength(14),
                    paddingRight    = new StyleLength(8),
                    paddingTop      = new StyleLength(4),
                    paddingBottom   = new StyleLength(4),
                    whiteSpace      = new StyleEnum<WhiteSpace>(WhiteSpace.NoWrap),
                    overflow        = new StyleEnum<Overflow>(Overflow.Hidden),
                    textOverflow    = new StyleEnum<TextOverflow>(TextOverflow.Ellipsis),
                },
            };

            row.RegisterCallback<PointerEnterEvent>(_ =>
                row.style.backgroundColor = new StyleColor(new Color(0.102f, 0.149f, 0.251f)));
            row.RegisterCallback<PointerLeaveEvent>(_ =>
                row.style.backgroundColor = StyleKeyword.Null);
            row.RegisterCallback<ClickEvent>(_ => OpenSearchTarget(path, 0, 0));

            return row;
        }

        private List<MdixSearchSource> CollectSearchSources()
        {
            var sources = new List<MdixSearchSource>();
            var seen    = new HashSet<string>(StringComparer.OrdinalIgnoreCase);

            foreach (var assetPath in AssetDatabase.GetAllAssetPaths())
            {
                if (!assetPath.EndsWith(".mdix", StringComparison.OrdinalIgnoreCase)) continue;
                if (!seen.Add(assetPath)) continue;

                // The open document is searched as it stands in the editor, not as it was last saved.
                if (string.Equals(assetPath, _currentPath, StringComparison.Ordinal))
                {
                    sources.Add(new MdixSearchSource(assetPath, _sourceText));
                    continue;
                }

                var physical = ResolvePhysicalPath(assetPath);
                if (physical == null || !File.Exists(physical)) continue;

                try   { sources.Add(new MdixSearchSource(assetPath, File.ReadAllText(physical))); }
                catch (IOException)                  { }
                catch (UnauthorizedAccessException)  { }
            }

            // A document that was never saved has no asset path, but it is still part of the work.
            if (string.IsNullOrEmpty(_currentPath) && !string.IsNullOrEmpty(_sourceText))
                sources.Add(new MdixSearchSource(UntitledKey, _sourceText));

            sources.Sort((a, b) => string.Compare(a.Path, b.Path, StringComparison.OrdinalIgnoreCase));
            return sources;
        }

        /// <summary>Where an asset path really is on disk. Package assets may live outside the project folder.</summary>
        private static string ResolvePhysicalPath(string assetPath)
        {
            if (assetPath.StartsWith("Assets/", StringComparison.Ordinal))
                return Path.GetFullPath(assetPath);

            if (assetPath.StartsWith("Packages/", StringComparison.Ordinal))
            {
                var info = UnityEditor.PackageManager.PackageInfo.FindForAssetPath(assetPath);
                if (info == null || string.IsNullOrEmpty(info.resolvedPath)) return null;

                var rest = assetPath.Substring(Math.Min(info.assetPath.Length, assetPath.Length)).TrimStart('/');
                return Path.Combine(info.resolvedPath, rest);
            }

            return null;
        }

        // ── Show ──────────────────────────────────────────────────────────────

        private void SetSearchSummary(string text, bool error)
        {
            if (_searchSummary == null) return;

            _searchSummary.text = text;
            _searchSummary.style.color = new StyleColor(error
                ? new Color(0.92f, 0.45f, 0.45f)
                : new Color(0.478f, 0.596f, 0.769f));
        }

        private void ShowSearchResults(MdixSearchResult result)
        {
            _searchResults.Clear();

            if (result.Error != null && result.Hits.Count == 0)
            {
                SetSearchSummary("✗  " + result.Error, error: true);
                return;
            }

            if (result.Hits.Count == 0)
            {
                SetSearchSummary(
                    "No matches in " + result.FilesSearched + (result.FilesSearched == 1 ? " file." : " files."),
                    error: false);
                return;
            }

            var summary = result.Hits.Count + " match" + (result.Hits.Count == 1 ? string.Empty : "es") +
                          " in " + result.FilesWithHits + (result.FilesWithHits == 1 ? " file" : " files");

            if (result.Truncated) summary += ". The list is cut here; narrow the search to see the rest";
            if (result.Error != null) summary += ". " + result.Error;

            SetSearchSummary(summary, error: result.Error != null);

            // Hits arrive grouped by file, so a header is added each time the file changes.
            var perFile = new Dictionary<string, int>(StringComparer.Ordinal);
            foreach (var hit in result.Hits)
            {
                int n;
                perFile.TryGetValue(hit.Path, out n);
                perFile[hit.Path] = n + 1;
            }

            string current = null;
            foreach (var hit in result.Hits)
            {
                if (!string.Equals(hit.Path, current, StringComparison.Ordinal))
                {
                    current = hit.Path;
                    var title = (hit.Path == UntitledKey ? "Untitled (not saved yet)" : hit.Path) +
                                "   " + perFile[hit.Path];
                    _searchResults.Add(MakeSectionHeader(title));
                }

                _searchResults.Add(MakeSearchRow(hit));
            }
        }

        private VisualElement MakeSearchRow(MdixSearchHit hit)
        {
            var row = new Label(RichSnippet(hit))
            {
                enableRichText = true,
                style =
                {
                    paddingLeft     = new StyleLength(14),
                    paddingRight    = new StyleLength(8),
                    paddingTop      = new StyleLength(3),
                    paddingBottom   = new StyleLength(3),
                    whiteSpace      = new StyleEnum<WhiteSpace>(WhiteSpace.NoWrap),
                    overflow        = new StyleEnum<Overflow>(Overflow.Hidden),
                    textOverflow    = new StyleEnum<TextOverflow>(TextOverflow.Ellipsis),
                },
            };

            row.RegisterCallback<PointerEnterEvent>(_ =>
                row.style.backgroundColor = new StyleColor(new Color(0.102f, 0.149f, 0.251f)));
            row.RegisterCallback<PointerLeaveEvent>(_ =>
                row.style.backgroundColor = StyleKeyword.Null);
            row.RegisterCallback<ClickEvent>(_ => OpenSearchHit(hit));

            return row;
        }

        /// <summary>The line with the match in bold gold, and the line:column in front.</summary>
        private static string RichSnippet(MdixSearchHit hit)
        {
            var text  = hit.LineText ?? string.Empty;
            var start = Math.Max(0, Math.Min(hit.MatchInLine, text.Length));
            var len   = Math.Max(0, Math.Min(hit.MatchLength, text.Length - start));

            var before = text.Substring(0, start).Replace("\t", "  ");
            var match  = text.Substring(start, len).Replace("\t", "  ");
            var after  = text.Substring(start + len).Replace("\t", "  ");

            var kind = MdixProjectSearch.KindLabel(hit.Class);

            return "<color=#6B7A90>" + (hit.Line + 1) + ":" + (hit.Column + 1) + "</color>   " +
                   Highlight.MdixRichText.Escape(before) +
                   "<b><color=#FFD27F>" + Highlight.MdixRichText.Escape(match) + "</color></b>" +
                   Highlight.MdixRichText.Escape(after) +
                   (kind.Length == 0 ? string.Empty : "   <color=#5C6B80>" + kind + "</color>");
        }

        // ── Open a result ─────────────────────────────────────────────────────

        private void OpenSearchHit(MdixSearchHit hit) => OpenSearchTarget(hit.Path, hit.Offset, hit.Length);

        /// <summary>Opens <paramref name="path"/> in the Editor tab with [offset, offset + length) selected.</summary>
        private void OpenSearchTarget(string path, int offset, int length)
        {
            if (_codeField == null) return;

            var sameDocument = path == UntitledKey
                ? string.IsNullOrEmpty(_currentPath)
                : string.Equals(path, _currentPath, StringComparison.Ordinal);

            if (!sameDocument)
            {
                var asset = AssetDatabase.LoadAssetAtPath<MdixAsset>(path);
                if (asset == null)
                {
                    SetStatus("✗  Could not open " + path, error: true);
                    return;
                }

                // Opening another file would drop unsaved edits, so this asks first.
                if (!ConfirmLeaveCurrentDocument(path)) return;

                LoadAsset(asset);
            }

            ShowTab(1);

            // The Editor tab has only just been shown; give it a moment before selecting.
            var from = Math.Max(0, Math.Min(offset, _sourceText.Length));
            var to   = Math.Max(from, Math.Min(offset + length, _sourceText.Length));

            _codeField.schedule.Execute(() =>
            {
                _codeField.Focus();
                SetSelection(to, from);
            }).ExecuteLater(40);
        }
    }
}
