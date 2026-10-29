# Changelog

All notable changes to the MDIX Unity package are documented here.
Format follows [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## [Unreleased]

### Added

- MDIX Studio: Autocomplete and Hover are separate switches (MidManStudio → MDIX Language Server,
  and the ⋮ menu on the window tab), next to Syntax Highlighting. Off means no requests are made.
  Compile errors and the problems list are not affected.
- MDIX Studio Search tab: **File names** mode lists the `.mdix` files whose name or folder matches
  (every file when empty); click one to open it. Remembered between sessions.
- MDIX Studio: **Copy** button next to Save, and right-click Copy on the status line and on each
  problem, so an error message can be pasted.
- MDIX Studio: **Strip Editor Colour Tags** (MidManStudio → MDIX Language Server) removes the
  `<color=#…>` markup that earlier builds could write into a file.
- MDIX Studio Explorer: edit plain values in place. Click a number, text, true/false or colour that
  is written out in the file, type, and press Enter (Esc cancels). The change is made in the
  Editor tab's text and compiled; the type is kept (`L`, `f`, a double stays a double), text is
  escaped, and the edit is refused unless the compiled value is exactly what was entered and the
  data keeps the same paths. Values calculated from the edited one may change and are named in
  the status bar. The list keeps its scroll position. Values from function calls, enum
  members, expressions, tuples and hex/binary numbers stay read-only.
  (`Editor/MdixSourceMap.cs`, `Editor/MdixLiteralEdit.cs`.)
- MDIX Studio Explorer: dates (`2026-01-15`), timestamps (`2025-01-15T10:30:00.250+05:30`), regex
  values (`r:("...")`) and `raw` backtick strings can be edited the same way. A timestamp keeps its
  zone and fraction digits, a date or timestamp is refused unless it is a real one, a regex is
  typed as the pattern and escaped for you, and a raw string cannot hold a backtick.
  (`Editor/Popups/MdixTemporalText.cs`.)
- MDIX Studio Explorer: popup editors. Click a date or timestamp to pick it from a month grid (with
  time boxes and a None / UTC / Offset zone for a timestamp, a text box for the whole value, and a
  Today / Now button). Click a raw string, or text longer than 100 characters or with line breaks,
  to edit it in a text area (Ctrl or Cmd + Enter applies). Esc cancels, and a refused edit is
  shown in the popup, which stays open. A popup changes the text through the same checked path as
  the inline field. (`Editor/Popups/`, `Editor/MdixEditorWindow.Popups.cs`.)
- MDIX Studio Explorer: click a regex to edit it with live matching. Type some test text and the
  matches and their groups are listed as you type, checked by the language server with the same
  Rust `regex` engine the compiler uses; an invalid pattern shows the engine's message and blocks
  Apply. If the server cannot be reached the check falls back to .NET's regex engine and says it
  is approximate (.NET accepts look-around and backreferences, which DixScript rejects); Apply
  still compiles the edit with the real engine first. The last test text is kept for the next
  regex popup. (`Editor/Popups/MdixRegexEditor.cs`, `Editor/Popups/MdixRegexTest.cs`,
  `MdixLspClient.RequestTestRegexAsync`.)
- MDIX Studio Explorer: click a blob (`b:("...")`) to see what it is and work with it. A PNG or JPEG
  is shown as an image, a WAV or Ogg Vorbis can be played (Unity's own decoder and editor preview
  player, through a temporary file outside the project that is deleted again), text is shown as
  text, and anything else shows its first bytes as a hex dump. **Replace from file** and **Clear**
  stage a change that **Apply** writes (a file over 1 MB is refused, since it goes into the text as
  base64), **Save as** writes the data to a file, and **Open** opens a temporary copy in the default
  app. Needs the built-in modules `unitywebrequestaudio` and `imageconversion` (both on by default).
  (`Editor/Popups/MdixBlobEditor.cs`, `MdixAudioPreview.cs`, `MdixBlobTemp.cs`.)
- MDIX Studio Explorer: blob and regex values show `blob  4.2 KB  PNG` and `r:(pattern)` instead
  of `?`. The type is read from the first bytes only, so a large blob costs nothing to show.
  (`Editor/Popups/MdixBlobInfo.cs`.)
- MDIX Studio language-server support, powered by the bundled `mdix-lsp`
  (`Editor/Bin/<platform>/`). In the Editor tab:
  - Live diagnostics: problem underlines after a short typing pause, and a
    problems list whose rows jump to the line.
  - Completion: opens on `@ . < ~ { ( [`, or on demand with Ctrl+Space
    (Cmd/Ctrl+I as an alternative). Up/Down/PageUp/PageDown to move, Enter or
    Tab to accept, Esc to dismiss. Snippet completions expand with their
    placeholders selected; Tab / Shift+Tab move between them.
  - Hover documentation for the symbol at the caret after a short pause.
  - A status line showing the real server state (starting, ready, number of
    problems, or the error).
  - Unsaved scratch documents are known to the server too (opened under an
    `untitled:` URI).
- Syntax highlighting in the Editor tab.
- **MidManStudio → MDIX Language Server** menu: Syntax Highlighting,
  Verbose Server Log, Restart, Set Server Path.... The server is looked up in
  this order: the path set from that menu, the `MDIX_LSP_PATH` environment
  variable, the binary bundled with the package, then `PATH`.
- Double-clicking a `.mdix` asset opens it in MDIX Studio.
- Explorer sections fold and unfold: click a section header, or use
  Fold All / Unfold All.
- **Save** on an unsaved scratch document now asks where to save it.
- **Search** tab: find text in every `.mdix` file of the project (Assets and
  packages). Plain or regex, optional match case, and **Names only** to skip
  values and comments. The open document is searched with its unsaved edits.
  Click a result to open the file with the match selected.
- The Explorer shows nested data to any depth: objects and arrays inside
  objects, arrays of arrays, and arrays of objects that hold more arrays. Every
  level folds. Arrays of flat objects stay a table.
- The bake wizard lists the `[MdixBakeable]` classes best fit first, with how
  much of the file each one reads. It tells you what the bake could not fill
  (members without data, keys nothing reads, members Unity would not store),
  refuses a bake that would put defaults where the data had a value, and
  updates an existing asset in place so references to it survive.
- Bake binder (`Runtime/MdixBinder.cs`): fills public fields,
  `[SerializeField]` fields and writable properties, lists, arrays, nested
  classes and enums to any depth.
- `MdixAsset.Bind<T>(dataPath)` and `MdixBinder.Create<T>(db, dataPath)`: the
  binder at runtime. Builds a plain class or struct and fills it, lists and
  arrays included, which `LoadAs<T>` cannot do. It fails only when data exists
  but cannot be converted. `MdixBinder.Create<T>(db, path, out report)` also
  returns a `MdixBindReport` (members without data, keys nothing reads), and
  `report.Describe()` prints it. The binder moved from the Editor assembly to
  Runtime, and `MdixBinder` and `MdixBindReport` are now public.
- The binder reads Unity's `Vector2`, `Vector3`, `Vector4`, `Vector3Int`,
  `Quaternion`, `Matrix4x4` and `Color32` from objects (`{ x = 1, y = 2, z = 3 }`)
  and `Color` from a hex colour.
- `MdixWriter.ToText(value, prefix)` and `MdixWriteReport`: the save side of the binder. Writes any class,
  struct or record as DixScript text: public fields, `[SerializeField]` fields and properties; every
  integer type, floats, decimal, char, enums (by name), `Guid`, `DateTime`, `DateTimeOffset`, `TimeSpan`;
  arrays, lists, sets and dictionaries of anything storable; and Unity's `Vector2/3/4`, `Vector2Int/3Int`,
  `Quaternion`, `Matrix4x4`, `Rect`, `RectInt`, `Bounds`, `BoundsInt`, `Color` and `Color32`. Members it
  cannot store (Unity object references, delegates, `object`, interfaces) are left out and listed in the
  report. The text is read back and checked before it is returned.
- The binder reads what the writer writes: dictionaries (as key/value entries, or a hand-written object),
  `HashSet<T>`, `byte[]` (base64), `char`, `Guid`, `DateTime`, `DateTimeOffset`, `TimeSpan`, `decimal`,
  NaN and infinities, `Rect` and `Bounds`, `Color` from `{ r, g, b, a }`, and `Color32` from a hex colour.
  Properties with a private setter are bound. Classes without a parameterless constructor (positional
  records) are created without running a constructor. `MdixBinder.ReadAs<T>(db, path)` reads any type,
  a list or dictionary included.

### Changed

- `MdixUnityExtensions.Save`, `LoadSave`, `SaveConfig` and `LoadConfig`, and `MdixAsset.LoadAs`, now use the
  writer and binder instead of the library serializer. Files written by earlier versions still load.
- Saves are written to `name.mdix.tmp` and swapped in, with the previous file kept as `name.mdix.bak`.
  `LoadSave` falls back to the backup when the file is missing, cut off or damaged, and `DeleteSave` removes
  both. A cut-off save is detected by its missing closing bracket; the engine itself would load the part
  that is there.

### Fixed

- Game code outside an assembly definition can use `MidManStudio.Mdix.Core`. The Core DLL was marked
  "explicitly referenced" in Unity, so only assemblies that list it by name could see it. A project
  with no assembly definitions got `CS0234` on `using MidManStudio.Mdix.Core;` and `CS0246` on
  `MdixDatabase`, which is the Quick Start in the README and the return type of `MdixAsset.Load()`.
  The DLL is now auto-referenced. The package's own assembly definitions list it by name and are not
  changed. (`Runtime/Plugins/MidManStudio.Mdix.Core.dll.meta`, and the template in
  `build-upm.yml` that writes this file when it is missing.)
- MDIX Studio: the highlighter's colour markup (`<color=#…>`) no longer reaches the text buffer.
  Setting the colour layer's text sent a change event that bubbled up to the code field, and the
  window took it for an edit. It went to the language server (hover reported hex colours that were
  not in the file; completion offered the wrong lists), to the compiler (`Expected section keyword,
  found: Symbol(<)` on a file that looked correct) and to Save (the markup ended up on disk).
  (`Editor/Highlight/MdixCodeOverlay.cs`, `Editor/MdixEditorWindow.Lsp.cs`.)
- MDIX Studio: Save now compiles, so the status line and the Explorer show the saved version, and a
  file that no longer compiles is reported as such instead of "Saved".
- Encrypted files are now first-class in the Editor. `.mdix.enc` imports as an `MdixEncAsset` with a violet **ENC**
  icon; `.mdix.key` gets a red **KEY** icon and an Editor-only stand-in object that never holds the key.
  (`Editor/MdixEncImporter.cs`, `Editor/MdixKeyImporter.cs`, `Editor/MdixKeyAsset.cs`, `Editor/Icons/mdix_icon_enc.png`,
  `Editor/Icons/mdix_icon_key.png`.)
- The Studio **Compile** button (and **Assets ▸ MDIX ▸ Compile and Encrypt**) now writes `<name>.mdix.enc` and
  `<name>.mdix.key` beside a source that declares `@DLM` compression or encryption, by loading the file the same way
  `mdix compile` does. Saving and opening still only parse. Every compile makes a new key. The first compile adds
  `*.mdix.key` and `*.mdix.key.meta` to `.gitignore`. (`Editor/MdixEncryption.cs`, `Editor/MdixEditorWindow.cs`,
  `Editor/CreateMdixMenuItems.cs`.)
- Build guard: a build now fails if it would include a `.mdix.key`, or the plaintext `.mdix` of an encrypted file
  (referenced by an enabled scene, or in `Resources` / `StreamingAssets`, through dependencies).
  **MidManStudio ▸ MDIX Encryption ▸ Check Build For Secrets** runs the same check on demand. (`Editor/MdixBuildGuard.cs`.)
- New `MdixKeys` (provide a key from your own code: `MdixKeys.Provide("secrets", () => key)`) and `MdixEncAsset`
  (`Load()`, `LoadAs<T>()`, `LoadWithKey()`, `LoadWithPassword()`, `LoadAsync(provider)`), so the `.enc` can ship in a
  build as a normal asset while the key never does. (`Runtime/MdixKeys.cs`, `Runtime/MdixEncAsset.cs`.)
- README: "Encrypted Data" is now "Encrypted Data & Secrets" (workflow, guardrails, where the key should come from,
  troubleshooting) and its examples use `MdixPaths.EncConfigFile` (they used `ConfigFile`, which is the `.mdix` path).
- MDIX Studio Editor tab: the mouse wheel, two-finger scroll and a vertical scrollbar now scroll the code.
  The field was in Unity's plain multiline layout, which only scrolls when the caret moves; it now sets
  `verticalScrollerVisibility = Auto`, which switches it to the ScrollView layout.
  (`Editor/MdixEditorWindow.cs`, `Editor/MdixEditorWindow.Lsp.cs`.)
- MDIX Studio: the code field scrolls with the mouse wheel, two fingers and a scrollbar. Unity only
  builds a multiline TextField's scrolling layout once its vertical scroller visibility is set, and it
  starts out Hidden; on 2022.3 that is the method `SetVerticalScrollerVisibility`, which was never
  called (the property of the same name does not exist there and failed to compile). The panels also
  took the height of their content, which pushed the problems list out of the window.
  (`Editor/MdixEditorWindow.cs`, `Editor/MdixEditorWindow.Lsp.cs`.)
- MDIX Studio: hover follows the mouse instead of the caret and sits below the pointer.
- MDIX Studio Search tab: the query box no longer stretches to half the panel.
- MDIX Studio "Generate ScriptableObject" window: it can be resized wider, and opens wide enough to
  read each row.
- MDIX Studio: section names are case-insensitive, as they are in the compiler. Highlighting colours
  `@Data` and `@data` like `@DATA` (they were left plain), and colours `@RAW` and `@SCHEMA`.
  Explorer inline editing finds `@Data(` in any case. (`Editor/Highlight/MdixTokenizer.cs`,
  `Editor/MdixSourceMap.cs`.)
- Saving silently lost data: lists, arrays, sets and dictionaries were written as their type name
  (`System.Collections.Generic.List`1[System.Int32]`) and loaded back empty, public fields were ignored,
  `uint`, `ulong`, `char` and `DateTimeOffset` came back wrong, `Guid` was not written, and a derived-class
  member was dropped. Saving any type with an indexer (every Unity `Vector`, `Quaternion`, `Matrix4x4` and
  `Color`) failed with "Parameter count mismatch". All of these now round-trip.

- **Generate ScriptableObject** left every list or array member empty, because
  the serializer it used cannot fill them, and it built the ScriptableObject
  with `new`, which Unity does not allow. Baking now fills the instance that
  `CreateInstance` made, from the engine's own values.
- The Explorer listed array items a second time as extra top-level rows, did
  not draw tables for arrays at the top of a file, and the status bar counted
  those items as "flat keys". The counts now match the entry count in the
  Inspector.
- The `[MdixBakeable]` documentation example could not work (a data path that
  already named the array, and properties Unity does not store). Corrected.

- The **Open in MDIX Studio** and **Generate ScriptableObject** buttons in the
  `.mdix` asset Inspector did nothing: the importer's read-only asset made
  Unity draw the Inspector disabled.
- Opening a file no longer marks it as modified, and the Editor tab is filled
  in when the window opens before its UI exists.
- Opening another file now asks first when there are unsaved edits, and
  choosing **MidManStudio → MDIX Studio** again no longer replaces them.
- **Compile** no longer marks the document as saved, so edits that were only
  compiled are not dropped silently when the window closes.
- Closing the window with an unsaved scratch document offered to save it and
  then silently discarded it.
- Added the missing `.meta` files for the `Highlight/` folder, the new scripts
  under `Highlight/` and `Lsp/`, and the `Editor/Bin/<platform>/` folders. A
  git-URL install ignores assets that have no `.meta`.
- Documentation: the Studio menu is **MidManStudio → MDIX Studio** (not
  Window), and the minimum Unity version is 2022.3, as `package.json` says
  (the README badge and the 1.0.0 note below said 2023.1).

## [1.0.0] — 2026-03-15

### Added

- `MdixAsset` — first-class `.mdix` Unity asset via `ScriptedImporter`.
  Drag into Inspector fields, double-click to open in MDIX Studio.

- `MdixEditorWindow` — MDIX Studio editor window with three tabs:
  - Explorer: compiled data viewer with flat properties and Supabase-style
    array tables. BOSS-tier enum rows highlighted in amber.
  - Editor: source text editor with compile-on-demand and save.
  - Templates: one-click creation of blank, enemies, items, config,
    server, encrypted secrets, and player save templates.

- `MdixBakeWizard` — right-click a `.mdix` asset to bake it into a typed
  Unity ScriptableObject. Searches project assemblies for `[MdixBakeable]`
  ScriptableObject subclasses.

- `[MdixBakeable]` attribute — marks a `ScriptableObject` subclass as a
  valid bake target. Accepts an optional `dataPath` and `displayName`.

- `MdixPaths` — centralized platform-correct path management.
  All mdix runtime data lives under `persistentDataPath/mdix/`:
  - `saves/`  — player save data
  - `config/` — mutable game config
  - `cache/`  — remote or compiled config cache
  - `.keys/`  — locally cached key files

- `MdixKeyStorage` — platform-appropriate key file storage and retrieval.
  Supports local sandbox storage, cloud key retrieval (HTTPS), cloud fetch
  with local cache, and custom `IMdixKeyProvider` implementations.

- `MdixUnityExtensions` — Unity-friendly helpers:
  - `LoadFrom(MdixAsset)` / `LoadAs<T>(MdixAsset)`
  - `LoadCoroutine` — Android StreamingAssets-aware coroutine loader
  - `LoadAsync` with main-thread callback dispatch
  - `Save<T>` / `LoadSave<T>` / `SaveExists` / `DeleteSave`
  - `SaveConfig<T>` / `LoadConfig<T>`

- `MdixInitializer` — `RuntimeInitializeOnLoadMethod` that creates the
  mdix directory structure automatically before the first scene loads.

- `CreateMdixMenuItems` — Assets → Create → MDIX menu with all templates.
  Right-click context menu: Generate ScriptableObject, Open in MDIX Studio.

- Native plugin binaries for Windows x64, Linux x64, macOS universal,
  Android arm64, and iOS (static library for IL2CPP).

- `link.xml` — prevents IL2CPP from stripping `MdixNative` P/Invoke calls
  on iOS and Android release builds.

- GitHub Actions CI workflow — builds Rust FFI and Core.dll on every push
  to master, assembles the package, pushes to the `upm` branch.

### Notes

- Minimum Unity version: 2023.1 LTS
- Binary serialization format is pending in the Rust crate. `MdixAsset`
  stores raw source text internally and parses on demand via the FFI.
  This will be updated to a compiled binary format in a future release.
