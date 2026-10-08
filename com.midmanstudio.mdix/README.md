# MidMan Studio — Mdix for Unity

**Structured, typed, encrypted game data.**  
More powerful than PlayerPrefs. Lighter than SQLite.

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Unity 2022.3+](https://img.shields.io/badge/Unity-2022.3%2B-black)](https://unity.com/)

---

## What is it?

Mdix brings the DixScript `.mdix` format to Unity. Think of it as the gap
between PlayerPrefs and a full database:

| | PlayerPrefs | **Mdix** | SQLite |
|---|---|---|---|
| Structured data | ❌ | ✅ | ✅ |
| Types beyond string/int/float | ❌ | ✅ | ✅ |
| Formulas and deduplication | ❌ | ✅ | ❌ |
| Built-in encryption | ❌ | ✅ | ❌ |
| Human-readable files | ❌ | ✅ | ❌ |
| LINQ-style queries | ❌ | ✅ | ✅ |
| Setup complexity | None | Low | High |

---

## Installation

Add via Unity Package Manager using the git URL:
```
https://github.com/Mid-D-Man/DixScript-Rust.git#upm
```

Or add to `Packages/manifest.json`:
```json
{
  "dependencies": {
    "com.midmanstudio.mdix": "https://github.com/Mid-D-Man/DixScript-Rust.git#upm"
  }
}
```

---

## Quick Start

### 1. Create a .mdix file

Right-click in the Project window → **Create → MDIX → Game Enemies**

### 2. Reference it in a MonoBehaviour
```csharp
using MidManStudio.Mdix.Unity;
using MidManStudio.Mdix.Core;
using UnityEngine;

public class EnemySpawner : MonoBehaviour
{
    [SerializeField] private MdixAsset _enemyData;

    void Start()
    {
        using var db = _enemyData.Load().OrThrow();

        var goblinHealth = db.GetInt("enemies[0].health").UnwrapOr(50);
        Debug.Log($"Goblin health: {goblinHealth}");
    }
}
```

### 3. Deserialize into a typed class
```csharp
[System.Serializable]
[MdixObject]
public class EnemyConfig
{
    public string Name   { get; set; }
    public int    Health { get; set; }
    public int    Damage { get; set; }
}

// In your MonoBehaviour:
var enemies = _enemyData
    .LoadAs<List<EnemyConfig>>("enemies")
    .UnwrapOr(new List<EnemyConfig>());
```

---

## Save System

Mdix replaces PlayerPrefs for structured save data:
```csharp
using MidManStudio.Mdix.Unity;

// Define your save data
[MdixObject]
public class PlayerSave
{
    public string PlayerName { get; set; } = "Player";
    public int    Level      { get; set; } = 1;
    public int    Health     { get; set; } = 100;
}

// Save
var data = new PlayerSave { PlayerName = "Hero", Level = 5, Health = 80 };
MdixUnityExtensions.Save("slot1", data);
// Writes to: persistentDataPath/mdix/saves/slot1.mdix

// Load
var save = MdixUnityExtensions
    .LoadSave<PlayerSave>("slot1")
    .UnwrapOr(new PlayerSave());

// Check and delete
bool exists = MdixUnityExtensions.SaveExists("slot1");
MdixUnityExtensions.DeleteSave("slot1");
```

---

## Encrypted Data

For sensitive game data (server configs, API keys, premium content):
```csharp
using MidManStudio.Mdix.Unity;

// Load encrypted file — key retrieved from your server at runtime.
// The key never touches the player's disk.
var db = await MdixKeyStorage.LoadWithCloudKeyAsync(
    MdixPaths.ConfigFile("server_settings"),
    "https://yourserver.com/api/keys/server_settings",
    cancellationToken);
```

For mobile with offline support:
```csharp
// Fetch key once when authenticated, cache locally in the app sandbox.
// Subsequent launches use the cached key without a network call.
var db = await MdixKeyStorage.LoadWithCloudKeyAndCacheAsync(
    MdixPaths.ConfigFile("premium_content"),
    "https://yourserver.com/api/keys/premium_content");
```

---

## Data Paths

All mdix runtime data lives under a single directory:
```
persistentDataPath/
└── mdix/
    ├── saves/      ← MdixPaths.SaveFile("slot1")
    ├── config/     ← MdixPaths.ConfigFile("difficulty")
    ├── cache/      ← MdixPaths.CacheFile("remote_items")
    └── .keys/      ← managed automatically by MdixKeyStorage
```

Bundled read-only game data (enemy tables, item definitions) goes in:
```
StreamingAssets/
└── mdix/           ← MdixPaths.StreamingFile("enemies.mdix")
```

---

## Bake to ScriptableObject

For data that never changes at runtime, bake your `.mdix` into a typed
Unity ScriptableObject for zero-cost access:
```csharp
// 1. Describe the data. Public fields are what Unity stores. A key such as
//    spawn_cap is read by a member called SpawnCap, spawnCap or spawn_cap.
[Serializable]
public class EnemyConfig
{
    public string Name;
    public int Health;
}

[MdixBakeable]
public class EnemyDatabase : ScriptableObject
{
    public int SpawnCap;
    public List<EnemyConfig> Enemies;
}

// 2. Right-click the .mdix asset in the Project window
//    → MDIX → Generate ScriptableObject
//    → the class that fits this file best is already selected
//    → Click Generate

// 3. Use the baked asset directly — no parsing, no FFI
public class Spawner : MonoBehaviour
{
    [SerializeField] private EnemyDatabase _enemies;

    void Start() => Debug.Log(_enemies.Enemies[0].Name);
}
```

What the bake reads: public fields and `[SerializeField]` fields, writable properties, lists and
arrays, nested `[Serializable]` classes and structs, and enums, to any depth. Enums are matched by
their number, so keep the numbers of a C# enum equal to those in the file's `@ENUMS` section.
Numbers convert between int, float and double; a value that does not fit refuses the bake instead
of being rounded.

After each bake the wizard lists the members the file has no value for, the keys nothing reads, and
the members Unity would not store (a property without `[field: SerializeField]`, a nested class that
is not `[Serializable]`). Baking again updates the same asset in place, so scenes and prefabs that
point at it keep working. The class list shows how much of the file each class reads, best first.

---

## MDIX Studio

Open via **MidManStudio → MDIX Studio**, double-click any `.mdix` asset, or right-click one and
choose **MDIX → Open in MDIX Studio**.

- **Explorer tab** — compiled data viewer. Plain values are key-value rows. An array of flat
  objects is a table with typed columns. Objects and arrays nest to any depth; each one folds when
  you click its header, and **Fold All** / **Unfold All** does every one at once. Large data is
  capped (200 items per array) and says so.
- **Editor tab** — source text editor with live compile status, syntax highlighting and
  language-server features (diagnostics, completion, hover — see below). **Save** on an
  unsaved scratch document asks where to put it.
- **Search tab** — find text in every `.mdix` file of the project, packages included. Typing
  searches after a short pause and Enter searches at once. **Match case**, **Regex** and
  **Names only** (keys, sections, enums and functions, skipping values and comments) narrow it. Click
  a result to open the file at the match. The open document is searched as you have it, unsaved
  edits included.
- **Templates tab** — create new files from built-in templates.

### Language server

The Editor tab talks to `mdix-lsp`, which ships inside the package (`Editor/Bin/<platform>/`), so
there is nothing to install.

| Feature | How it works |
|---|---|
| Diagnostics | Problem underlines appear after a short typing pause; click a row in the problems list to jump to that line |
| Completion | Opens on `@ . < ~ { ( [`; **Ctrl+Space** (or **Cmd/Ctrl+I**) opens it on demand. Up/Down/PageUp/PageDown to choose, **Enter** or **Tab** to accept, **Esc** to dismiss |
| Snippets | After accepting one, **Tab** / **Shift+Tab** move between its placeholders; **Esc** ends it |
| Hover | Leave the caret still for a moment and the docs for the symbol under it appear |
| Highlighting | Toggle under **MidManStudio → MDIX Language Server → Syntax Highlighting** |

The Editor tab shows the server's state (starting, ready, how many problems, or the error). The
**MidManStudio → MDIX Language Server** menu also has **Restart**, **Verbose Server Log** and
**Set Server Path...**. The server is looked up in this order: the path set from that menu, the
`MDIX_LSP_PATH` environment variable, the bundled binary, then your `PATH`.

---

## Platform Support

| Platform | Status |
|---|---|
| Windows x64 | ✅ |
| Linux x64 | ✅ |
| macOS (Universal) | ✅ |
| Android arm64 | ✅ |
| iOS | ✅ (static library) |
| WebGL | ⚠️ Not supported — no native plugin support |

---

## License

MIT — see [LICENSE](https://github.com/Mid-D-Man/DixScript-Rust/blob/master/LICENSE)
```

---

That's the complete package. Here's a summary of every file delivered across all responses so you have a single reference:
```
com.midmanstudio.mdix/
├── package.json
├── CHANGELOG.md
├── README.md
│
├── Runtime/
│   ├── MidManStudio.Mdix.Runtime.asmdef
│   ├── MidManStudio.Mdix.Runtime.asmdef.meta
│   ├── link.xml
│   ├── MdixAsset.cs
│   ├── MdixBakeableAttribute.cs
│   ├── MdixInitializer.cs
│   ├── MdixKeyStorage.cs
│   ├── MdixPaths.cs
│   ├── MdixUnityExtensions.cs
│   └── Plugins/
│       ├── MidManStudio.Mdix.Core.dll        ← CI-populated
│       ├── MidManStudio.Mdix.Core.dll.meta
│       ├── Windows/x86_64/
│       │   ├── mdix_ffi.dll                  ← CI-populated
│       │   └── mdix_ffi.dll.meta
│       ├── Linux/x86_64/
│       │   ├── libmdix_ffi.so                ← CI-populated
│       │   └── libmdix_ffi.so.meta
│       ├── macOS/
│       │   ├── libmdix_ffi.dylib             ← CI-populated
│       │   └── libmdix_ffi.dylib.meta
│       ├── Android/arm64-v8a/
│       │   ├── libmdix_ffi.so                ← CI-populated
│       │   └── libmdix_ffi.so.meta
│       └── iOS/
│           ├── libmdix_ffi.a                 ← CI-populated
│           └── libmdix_ffi.a.meta
│
├── Editor/
│   ├── MidManStudio.Mdix.Editor.asmdef
│   ├── MidManStudio.Mdix.Editor.asmdef.meta
│   ├── MdixImporter.cs
│   ├── MdixAssetEditor.cs                    ← inside MdixImporter.cs
│   ├── MdixBakeWizard.cs
│   ├── MdixEditorWindow.cs
│   ├── CreateMdixMenuItems.cs
│   └── UI/
│       ├── MdixEditorWindow.uxml
│       ├── MdixEditorWindow.uxml.meta
│       ├── MdixEditorWindow.uss
│       └── MdixEditorWindow.uss.meta
│
└── .github/
    └── workflows/
        └── build-upm.yml
