# Changelog

All notable changes to the MDIX Unity package are documented here.
Format follows [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

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
