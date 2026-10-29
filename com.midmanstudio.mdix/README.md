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

`LoadAs` reads public fields, `[SerializeField]` fields and properties, and fills lists, arrays, sets,
dictionaries, nested classes and enums to any depth. `Bind<T>` does the same with the path of the data
as its argument:
```csharp
[System.Serializable]
public class EnemyConfig { public string Name; public int Health; }

public class EnemyTable
{
    public int SpawnCap;
    public List<EnemyConfig> Enemies;
}

var result = _enemyData.Bind<EnemyTable>();          // whole file; Bind<T>("server") for one object
if (result.IsSuccess) Debug.Log(result.SuccessResult.Enemies.Count);
else                  Debug.LogError(result.Error.Message);
```
A key such as `spawn_cap` is read by a member called `SpawnCap`, `spawnCap` or `spawn_cap`. Unity's
`Vector2`, `Vector3`, `Vector4`, `Vector2Int`, `Vector3Int`, `Quaternion`, `Matrix4x4`, `Rect` and `Bounds` are
read from objects such as `{ x = 1.0, y = 2.0, z = 3.0 }`, and `Color` and `Color32` from a hex colour such as
`#FF8000FF` (a `Color` also from `{ r = 1.0, g = 0.5, b = 0.0, a = 1.0 }`). `Bind<T>`
fails only when data exists but cannot be converted; a member with no data keeps the value its
constructor gave it. For a report of members without data and keys nothing reads, call
`MdixBinder.ReadAs<T>(db, "", out var report)` on a database you loaded yourself and print
`report.Describe()`. `T` is a class or struct (a positional record works) or a ScriptableObject type; to fill
an instance you already have, use `MdixBinder.Bind(db, instance, "")`.

`Bind<T>` works by reflection. On IL2CPP builds (iOS, Android, WebGL, consoles) keep your data
classes from being stripped: mark them `[UnityEngine.Scripting.Preserve]`, or add their assembly to
a `link.xml` in your project (`<assembly fullname="YourAssembly" preserve="all"/>`). The package's
own `link.xml` already preserves the Mdix assemblies.

---

## Save System

Mdix replaces PlayerPrefs for structured save data. Any class or struct works as it is: no attributes,
no conversion code, no splitting a `Vector3` into three floats.
```csharp
using MidManStudio.Mdix.Unity;

public class PlayerSave
{
    public string Name = "Player";
    public int Level = 1;
    public Vector3 Position;
    public Quaternion Rotation;
    public Color Tint = Color.white;
    public List<ItemStack> Items = new List<ItemStack>();
    public Dictionary<string, int> Flags = new Dictionary<string, int>();
    public DateTime LastSaved;
}

// Save
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

The file is plain text you can read and edit:
```
@DATA(
  name = "Kai"
  level = 3
  position = { x = 1.25f, y = -2.0f, z = 3.5f }
  rotation = { x = 0.1f, y = 0.2f, z = 0.3f, w = 0.9f }
  tint = { r = 1.0f, g = 0.5f, b = 0.25f, a = 1.0f }
  items = [{ id = "sword", count = 1 }, { id = "potion", count = 5 }]
  flags = [{ key = "door_open", value = 1 }]
  last_saved = "2026-10-10T08:00:00.0000000Z"
)
```

**What is stored.** Public fields, `[SerializeField]` private fields, and properties with a public getter
and any setter (`{ get; private set; }` included). Mark a field `[NonSerialized]` or a property
`[MdixIgnore]` to leave it out.

| Kind | Types |
|---|---|
| Numbers and text | `int`, `uint`, `long`, `ulong`, `short`, `ushort`, `byte`, `sbyte`, `float`, `double`, `decimal`, `bool`, `char`, `string`, enums (by name), nullables |
| Time and ids | `DateTime` (its Kind is kept), `DateTimeOffset`, `TimeSpan`, `Guid` |
| Collections | arrays, `List<T>`, `HashSet<T>`, `Dictionary<K, V>` with any key type, any nesting of them, `byte[]` (as base64) |
| Your types | classes, structs and records (positional ones too), nested up to 32 levels |
| Unity | `Vector2`, `Vector3`, `Vector4`, `Vector2Int`, `Vector3Int`, `Quaternion`, `Matrix4x4`, `Rect`, `RectInt`, `Bounds`, `BoundsInt`, `Color`, `Color32` |

**What is not stored.** References to Unity objects (`GameObject`, `Transform`, `Sprite`, a
`ScriptableObject` reference: save a name or an id instead), delegates and events, members typed `object`,
as an interface or as an abstract class, multi-dimensional arrays, and references that loop back on
themselves. These are left out and a warning names each one once. A derived object kept in a base-typed
member is saved with the base members only. Classes need a parameterless constructor or to be a record;
members missing from a file keep the value the constructor gave them.

**Safe writes.** The text is checked before anything is touched, then written to `slot1.mdix.tmp` and swapped
in, and the previous save is kept as `slot1.mdix.bak`. If the main file is missing, cut off or damaged,
`LoadSave` reads the backup and says so in the log. A failed save never replaces a good one.

**Slots and prefixes.** `Save("all", data, "slot2")` stores the object under the key `slot2` (`"a.b"` nests
further), so one file can hold several saves; pass the same prefix to `LoadSave`. A class marked
`[MdixObject("player")]` uses that prefix when you pass none.

`Save`, `LoadSave`, `SaveConfig` and `LoadConfig` are thin wrappers. `MdixWriter.ToText(data)` gives you the
text (and a report of what was not stored) and `MdixBinder.ReadAs<T>(db, path)` reads it back, if you want
to keep saves somewhere else (cloud, a database field). Both use reflection: on IL2CPP keep your save classes
from being stripped (`[UnityEngine.Scripting.Preserve]` or a `link.xml` entry). Expect on the order of a tenth
of a millisecond per stored object on a desktop CPU, several times that on a phone.

Files written by earlier versions of the package still load.

---

## Encrypted Data & Secrets

For anything you do not want players reading (API keys, server addresses, premium content). Three files
are involved, and each one has its own icon in the Project window:

| File | Icon | What it is | Ships in the build? |
|------|------|------------|---------------------|
| `secrets.mdix` | blue | Your readable source. | **No.** It is the thing being hidden. |
| `secrets.mdix.enc` | violet, **ENC** | The encrypted (and compressed) data. | **Yes.** |
| `secrets.mdix.key` | red, **KEY** | The decryption key. | **Never.** |

The point of the setup: the `.enc` goes out with the game, the key does not, and the only thing that can
open the data at runtime is code **you** wrote, which hands the key over.

### 1. Make the files

1. **Create → MDIX → Encrypted Secrets**. The template has an `@DLM` section that asks for compression
   and AES-256 encryption.
2. Fill in your values in **MDIX Studio**.
3. Press **Compile** in the Studio (or right-click the `.mdix` → **MDIX → Compile and Encrypt**).
   This writes `secrets.mdix.enc` and `secrets.mdix.key` next to the source.

Two things to know:

- **Only Compile generates.** Saving or opening a file just parses it. Every compile makes a **new random
  key**, so the `.enc` and `.key` are always a pair: if you compile again, ship the new `.enc` and
  update the key wherever you keep it. An old key does not open a new `.enc`.
- The files are written read-only. Compiling again replaces them.

### 2. The guardrails

- **gitignore.** The first compile adds `*.mdix.key` and `*.mdix.key.meta` to your project's `.gitignore`
  (it creates the file if the project is a git repository). If you use another version control system,
  ignore those two patterns yourself. A key that was committed *before* this was set up stays in git
  history: rotate it by compiling again.
- **Build guard.** A build fails, with the file names listed, if it would include a `.mdix.key`, or the
  plaintext `.mdix` of an encrypted file. "Include" means referenced by an enabled scene, or in a
  `Resources` folder or `StreamingAssets`, followed through dependencies. Run it any time from
  **MidManStudio → MDIX Encryption → Check Build For Secrets**. It does not look inside Addressables
  groups or AssetBundles you build yourself: keep keys and encrypted sources out of those.
- **The key object holds nothing.** In the Project window a `.mdix.key` is an Editor-only stand-in; the
  key text is never copied into the imported asset, so it cannot reach a build through it.
- **Keep the source out of your game.** Do not drag `secrets.mdix` into a scene or prefab field, and do
  not put it in `Resources` or `StreamingAssets`. A `MdixAsset` stores the whole source text in the build.

### 3. Ship the `.enc` and provide the key

Drag `secrets.mdix.enc` into a field. It is an `MdixEncAsset`: the encrypted bytes, included in the build
on every platform, with no file path to resolve. Then give it the key from your own code:

```csharp
using MidManStudio.Mdix.Unity;
using UnityEngine;

public class SecretsLoader : MonoBehaviour
{
    [SerializeField] private MdixEncAsset _secrets;      // drag secrets.mdix.enc here

    private async void Start()
    {
        // YOUR code decides where the key comes from (see below).
        string keyText = await MyBackend.FetchKeyAsync("secrets");
        MdixKeys.Provide(_secrets.KeyName, keyText);      // KeyName is "secrets"

        var result = _secrets.Load();
        MdixKeys.Revoke(_secrets.KeyName);                // optional: don't keep the key in memory

        if (result.IsFailure) { Debug.LogError(result.Error.Message); return; }

        using var db = result.SuccessResult;
        var apiKey = db.GetString("api_key").UnwrapOr("");
    }
}
```

`MdixKeys.Provide(name, text)` takes the text of the `.mdix.key` file. There is also
`Provide(name, () => text)`, which calls your function each time the key is needed, so you can decode or
fetch it inside the function instead of keeping it in a field. If no key was provided, `Load()` fails with a
message that says exactly that. The rest of the loaders:

| Call | Use when |
|------|----------|
| `asset.Load()` | You called `MdixKeys.Provide` first. |
| `asset.LoadAs<T>(prefix)` | Same, and you want it bound straight into a class. |
| `asset.LoadWithKey(keyText)` | You already hold the key text. |
| `asset.LoadAsync(provider)` | Your own `IMdixKeyProvider` fetches it (server, vault, secure enclave). The provider is passed the key name. |
| `asset.LoadWithPassword(keyText, password)` | Password-mode files (below). |

**Where should the key come from?** That is the one decision MDIX leaves to you, because it is the one that
matters:

- **Your server** (strongest). Fetch the key after login or at startup. It never exists on the player's disk
  and you can refuse to give it out. This is the right choice for anything valuable.
- **Per player.** Derive or deliver a key tied to the account.
- **Built into your code** (weakest). Better than a loose file, but anything the game can compute, someone
  patient can extract from the game. Treat it as keeping honest players and casual file browsing out, not
  as protection against a determined attacker.

Nothing that runs on a player's device can be made impossible to extract. If a secret must stay secret
(a server admin token, a payment key), do not ship it at all: keep it on your server.

### 4. Password mode and other options

- **Password mode.** A key file can be password-protected so the password, not a file, is the secret. The
  Studio cannot compile these yet (it has no password prompt); compile them with the `mdix` command line
  tool, then load with `LoadWithPassword(keyText, password)`.
- **Files on disk instead of assets.** For data you download or save at runtime (not shipped), the
  file-based helpers still work. The paths for encrypted files are `MdixPaths.EncConfigFile(...)` and
  `MdixPaths.EncSaveFile(...)`:

```csharp
// Key fetched from your server each launch; it never touches the player's disk.
var db = await MdixKeyStorage.LoadWithCloudKeyAsync(
    MdixPaths.EncConfigFile("server_settings"),
    "https://yourserver.com/api/keys/server_settings",
    cancellationToken);

// Mobile with offline support: fetch once, cache the key in the app sandbox.
var db = await MdixKeyStorage.LoadWithCloudKeyAndCacheAsync(
    MdixPaths.EncConfigFile("premium_content"),
    "https://yourserver.com/api/keys/premium_content");
```

### Troubleshooting

| You see | Cause and fix |
|---------|---------------|
| "No key was provided for 'secrets'" | Call `MdixKeys.Provide("secrets", …)` before `Load()`. The name is the file name without `.mdix.enc`. |
| Decryption fails after a recompile | Every compile makes a new key. Ship the new `.enc` **and** use the new key. |
| Build stops with "would ship a secret" | A `.mdix.key` or encrypted plaintext `.mdix` is referenced or sits in `Resources` / `StreamingAssets`. The message lists the files. |
| The key shows up in `git status` | It was tracked before it was ignored. `git rm --cached secrets.mdix.key`, then compile again for a fresh key. |
| Compile says it needs a password | The file uses password mode. Compile it with the `mdix` command line tool. |
| An unrelated `.enc` or `.key` file looks odd in the Project window | MDIX claims the `.enc` and `.key` extensions. Other files with those extensions are imported as harmless stand-ins and are not modified. |

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

  **Editing values in place.** A value written out in the file as a plain number, text, `true` /
  `false`, colour, date, timestamp, regex, blob or `raw` string is highlighted when you hover it. Click it, type, and press Enter: the change
  is made in the text of the Editor tab and compiled, and Esc (or clicking away) cancels. A number
  keeps its type (a `Long` keeps its `L`, a `Float` its `f`, a `Double` stays a double), and text is
  escaped for you. A timestamp keeps its zone and fraction digits, and a regex is edited as the
  pattern itself (a backslash is written as `\\` in the file for you). The new text is compiled before it is accepted, and the edit is refused unless
  the value reads back exactly as entered and the data keeps the same paths. Values calculated
  from the one you changed are allowed to move, and the status bar names them. Nothing is written to
  disk until **Save**.

  A date or timestamp opens a **picker**: a month grid (use « ‹ › » to move, or type the whole
  value in the box at the top), and for a timestamp the time boxes and a zone of None, UTC or an
  offset. **Today** / **Now** fills in the current date or time. A **regex** opens an editor with a
  test-text box: as you type, the matches and their groups are listed, checked by the language
  server with the same `regex` engine the compiler uses (so an invalid pattern is explained and
  cannot be applied). Without the server the check falls back to .NET's engine and is marked
  approximate. A **blob** opens a viewer: a PNG or JPEG is shown, a WAV or Ogg can be played, text is
  shown as text, and anything else as a hex dump. **Replace from file** (up to 1 MB) and **Clear**
  stage a change that **Apply** writes; **Save as** writes the data to a file, and **Open** shows a
  temporary copy in the default app. A `raw` string, and any text that
  is longer than 100 characters or has line breaks, opens a **text area** (Ctrl or Cmd + Enter
  applies). In a popup Enter applies, Esc cancels, and a refused edit is explained in the popup,
  which stays open.

  Only plain literals can be edited: a value produced by a function call (the `createEnemy("Goblin",
  50, ...)` kind), an enum member, an expression, a tuple, `null` or a hex or binary number stays
  read-only, because there is no single piece of text behind it that a field can hold. Change those
  in the Editor tab, where a function argument is the thing to edit.
- **Editor tab** — source text editor with live compile status, syntax highlighting and
  language-server features (diagnostics, completion, hover — see below). **Save** on an
  unsaved scratch document asks where to put it.
- **Search tab** — two modes, switched by **File names** (remembered). With it on, the tab lists
  the `.mdix` files whose name or folder matches what you type, and every file when the box is
  empty, so it also works as a file browser. With it off, it finds text inside every `.mdix` file of
  the project, packages included. Typing searches after a short pause and Enter searches at once.
  **Match case** and **Regex** apply to both; **Names only** (keys, sections, enums and functions,
  skipping values and comments) applies to searching inside files. Click a result to open the file
  (at the match, when searching inside). The open document is searched as you have it, unsaved
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
| Hover | Rest the mouse on a symbol and its docs appear below the pointer; move away and they go |
| Highlighting, Autocomplete, Hover | Separate switches under **MidManStudio → MDIX Language Server**, and in the **⋮** menu on the window tab. Off means off. Compile errors and the problems list always stay on |

The Editor tab shows the server's state (starting, ready, how many problems, or the error). The
**MidManStudio → MDIX Language Server** menu also has **Restart**, **Verbose Server Log** and
**Set Server Path...**, and **Strip Editor Colour Tags** for files that an earlier build saved with
`<color=#…>` markup in them. **Copy** (next to Save) copies the status line and every problem.
The server is looked up in this order: the path set from that menu, the
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
│   ├── MdixEditorWindow.Explorer.cs          ← tree view, folding, in-place editing
│   ├── MdixSourceMap.cs                      ← where each plain literal sits in the text
│   ├── MdixEditorWindow.Popups.cs            ← opens the value popups from the Explorer
│   ├── MdixLiteralEdit.cs                    ← formats, applies and checks one edit
│   ├── Popups/
│   │   ├── MdixPopup.cs                      ← the modal card: Apply, Cancel, error line
│   │   ├── MdixDateTimePicker.cs             ← month grid, time boxes and zone
│   │   ├── MdixDateTimeModel.cs              ← what the picker edits (no Unity types)
│   │   ├── MdixRegexEditor.cs                ← pattern box, test text, match list
│   │   ├── MdixRegexTest.cs                  ← runs and reads regex tests (no Unity types)
│   │   ├── MdixTemporalText.cs               ← reads and writes date and timestamp text
│   │   ├── MdixBlobEditor.cs                 ← blob viewer: preview, replace, save, open
│   │   ├── MdixAudioPreview.cs               ← plays a WAV or Ogg blob in the editor
│   │   ├── MdixBlobTemp.cs                   ← temporary copies outside the project
│   │   └── MdixBlobInfo.cs                   ← names a blob's type, hex dump, text preview
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
