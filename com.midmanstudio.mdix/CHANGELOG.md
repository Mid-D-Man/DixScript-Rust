# Changelog

All notable changes to the MDIX Unity package are documented here.
Format follows [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## [Unreleased]

### Added

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

### Fixed

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
