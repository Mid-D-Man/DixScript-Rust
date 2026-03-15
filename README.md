# MidMan Studio — Mdix for Unity

**Structured, typed, encrypted game data.**  
More powerful than PlayerPrefs. Lighter than SQLite.

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Unity 2023.1+](https://img.shields.io/badge/Unity-2023.1%2B-black)](https://unity.com/)

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
// 1. Mark your ScriptableObject
[MdixBakeable("enemies")]
public class EnemyDataAsset : ScriptableObject
{
    public List<EnemyConfig> enemies;
}

// 2. Right-click the .mdix asset in the Project window
//    → Generate ScriptableObject
//    → Pick EnemyDataAsset from the list
//    → Click Generate

// 3. Use the baked asset directly — no parsing, no FFI
public class Spawner : MonoBehaviour
{
    [SerializeField] private EnemyDataAsset _enemies;

    void Start()
    {
        var goblin = _enemies.enemies[0];
        Debug.Log(goblin.Name);
    }
}
```

---

## MDIX Studio

Open via **Window → MDIX Studio** or double-click any `.mdix` asset.

- **Explorer tab** — compiled data viewer. Flat properties shown as
  key-value rows. Arrays shown as Supabase-style tables with typed columns.
- **Editor tab** — source text editor with live compile status.
- **Templates tab** — create new files from built-in templates.

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
