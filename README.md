# DixScript: The Swiss Army Knife of Data Formats

**Config, Code, and Crypto in One `.mdix` File**

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/rust-1.70+-orange.svg)](https://www.rust-lang.org/)
[![Status](https://img.shields.io/badge/status-in_development-red.svg)]()

> **"I built this because I was tired of copy-pasting the same JSON config blocks 500 times. Turns out other people hate that too."**  
> — Mid-D-Man, Creator

---

## ⚠️ DO NOT USE IN PRODUCTION — ACTIVE DEVELOPMENT ⚠️

**None of the packages in this repository are ready for use yet.** Everything here is under active development and subject to breaking changes at any time. This includes:

- `dixscript` (core Rust library) — parser and runtime incomplete
- `mdix-cli` — not feature-complete
- `mdix-ffi` / C# NuGet package — API may change
- `mdix-go` — binds to an incomplete runtime
- `mdix-java` / `mdix-python` — binds to an incomplete runtime
- `mdix-wasm` — not published
- `mdix-c` — header not stable

**Do not `cargo add`, `pip install`, `go get`, or otherwise depend on any of these packages yet.** Watch the repo for a v1.0.0 release announcement. The C# prototype (`https://github.com/Mid-D-Man/DixScript`) is the only currently functional reference implementation.

---

## The Origin Story (Or: How Scope Creep Turned Into a Format)

**Started as:** A quick hack to make a mobile game's remote config less painful.

**The Problem:** The game had weapons, camos, attainment missions, and shop data spread across Unity Remote Config as massive nested JSON blobs. Adding a single new field to a camo definition meant updating it in dozens of places. One typo and suddenly every weapon in the game was broken at runtime — with no error until the player hit that screen.

Here's a real slice of what that looked like:

```json
{
  "EquipableItemCamoClassId": "ALL_SMG_CAMOS_CONFIG",
  "InventoryItemCamos": [
    {
      "MainItemId": "ALIYAHOO419",
      "MainItemClass": "BASIC_SMG",
      "CamoRaritySubClass": [
        {
          "RaritySubClassId": "Aliyahoo419_Basic_Camos",
          "MainItemCamos": [
            {
              "CamoId": "ALIYAHOO419",
              "CamoIndex": 0,
              "CamoAvailableInSeason": "1",
              "CamoRarity": "Basic",
              "CamoAtlasSpriteName": "Aliyahoo419(Clone)",
              "CamoInGameName": "Aliyahoo419",
              "CamoType": "Sprite",
              "MaterialAddress": "Null"
            }
          ]
        },
        {
          "RaritySubClassId": "Aliyahoo419_Epic_Camos",
          "MainItemCamos": [
            {
              "CamoId": "ALIYAHOO419_HORIZON",
              "CamoIndex": 0,
              "CamoAvailableInSeason": "1",
              "CamoRarity": "Rare",
              "CamoAtlasSpriteName": "Aliyahoo419_Horizon(Clone)",
              "CamoInGameName": "Aliyahoo419-Horizon",
              "CamoType": "Sprite",
              "MaterialAddress": "Null"
            },
            {
              "CamoId": "ALIYAHOO419_ROSE",
              "CamoIndex": 1,
              "CamoAvailableInSeason": "1",
              "CamoRarity": "Epic",
              "CamoAtlasSpriteName": "Aliyahoo419_Rose(Clone)",
              "CamoInGameName": "Aliyahoo419-Rose",
              "CamoType": "Sprite",
              "MaterialAddress": "Null"
            }
            // ... 3 more camos with the same 8-field pattern ...
          ]
        }
      ]
    }
    // ... 5 more weapons, each with 1–10 camos, each repeating all 8 fields ...
  ]
}
```

This is just the **camo config** for one weapon class. There's a separate blob for attainment configs, and another for unique camo missions. All three are duplicating the same weapon IDs, camo IDs, and structural boilerplate across hundreds of lines. Every Remote Config update is a manual find-and-replace exercise waiting to go wrong.

**The Solution:** "What if the shape of a camo was defined once, and I just filled in the data?"

**What Happened Next:** Classic developer move. Instead of using an existing tool, the format got built. Then features got added. Then cut. Then the syntax got rewritten three times. Scope creep happened. Hard.

**The Result:** The same SMG camo config, in DixScript:

```dixscript
@ENUMS(
  WeaponClass { BASIC_SMG, RUNIC_SMG, LEGENDARY_SMG }
  CamoRarity  { Basic, Rare, Epic, Legendary, Runic }
  CamoType    { Sprite, SpriteAndMaterial }
)

@QUICKFUNCS(
  // The shape of every camo — defined exactly once.
  // Change CamoAvailableInSeason across the whole game? Edit one line here.
  // Add a new field to every camo object? Add it here, done.
  ~camo<object>(id, index<int>, rarity<enum>, sprite, inGameName, type<enum>) {
    return {
      CamoId                = id
      CamoIndex             = index
      CamoAvailableInSeason = "1"
      CamoRarity            = rarity
      CamoAtlasSpriteName   = $"{sprite}(Clone)"
      CamoInGameName        = inGameName
      CamoType              = type
      MaterialAddress       = "Null"
    }
  }

  ~rarityClass<object>(subClassId, camos) {
    return { RaritySubClassId = subClassId, MainItemCamos = camos }
  }

  ~weapon<object>(itemId, class<enum>, rarityClasses) {
    return { MainItemId = itemId, MainItemClass = class, CamoRaritySubClass = rarityClasses }
  }
)

@DATA(
  EquipableItemCamoClassId = "ALL_SMG_CAMOS_CONFIG"

  InventoryItemCamos::
    weapon("ALIYAHOO419", WeaponClass.BASIC_SMG, [
      rarityClass("Aliyahoo419_Basic_Camos", [
        camo("ALIYAHOO419", 0, CamoRarity.Basic, "Aliyahoo419", "Aliyahoo419", CamoType.Sprite)
      ]),
      rarityClass("Aliyahoo419_Epic_Camos", [
        camo("ALIYAHOO419_HORIZON",      0, CamoRarity.Rare, "Aliyahoo419_Horizon",      "Aliyahoo419-Horizon",      CamoType.Sprite),
        camo("ALIYAHOO419_ROSE",         1, CamoRarity.Epic, "Aliyahoo419_Rose",         "Aliyahoo419-Rose",         CamoType.Sprite),
        camo("ALIYAHOO419_UNDERCAMO",    2, CamoRarity.Epic, "Aliyahoo419_UnderCamo",    "Aliyahoo419-UnderCamo",    CamoType.Sprite),
        camo("ALIYAHOO419_DARKNIGHTSKY", 3, CamoRarity.Epic, "Aliyahoo419_DarkNightSky", "Aliyahoo419-DarkNightSky", CamoType.SpriteAndMaterial),
        camo("ALIYAHOO419_GOLDENLINE",   4, CamoRarity.Epic, "Aliyahoo419_GoldenLine",   "Aliyahoo419-GoldenLine",   CamoType.Sprite)
      ])
    ]),

    weapon("BEATLE", WeaponClass.BASIC_SMG, [
      rarityClass("Beatle_Basic_Camos", [
        camo("BEATLE", 0, CamoRarity.Basic, "Beatle", "Beatle", CamoType.Sprite)
      ]),
      rarityClass("Beatle_Rare_Camos", [
        camo("BEATLE_BLOODYCHESS", 0, CamoRarity.Rare, "Beatle_BloodyChess", "Beatle-BloodyChess", CamoType.Sprite)
      ]),
      rarityClass("Beatle_Epic_Camos", [
        camo("BEATLE_ZAWOOD",       0, CamoRarity.Epic, "Beatle_ZaWood",       "Beatle-ZaWood",       CamoType.Sprite),
        camo("BEATLE_UNDERCAMO",    1, CamoRarity.Epic, "Beatle_UnderCamo",    "Beatle-UnderCamo",    CamoType.Sprite),
        camo("BEATLE_CRYSTALISED",  2, CamoRarity.Epic, "Beatle_Crystalised",  "Beatle-Crystalised",  CamoType.Sprite),
        camo("BEATLE_WAVYDRIP",     3, CamoRarity.Epic, "Beatle_WavyDrip",     "Beatle-WavyDrip",     CamoType.Sprite),
        camo("BEATLE_DARKNIGHTSKY", 4, CamoRarity.Epic, "Beatle_DarkNightSky", "Beatle-DarkNightSky", CamoType.SpriteAndMaterial),
        camo("BEATLE_GOLDENLINE",   5, CamoRarity.Epic, "Beatle_GoldenLine",   "Beatle-GoldenLine",   CamoType.Sprite)
      ])
    ]),

    weapon("KIG88", WeaponClass.BASIC_SMG, [
      rarityClass("KiG88_Basic_Camos", [
        camo("KIG88", 0, CamoRarity.Basic, "KiG88", "KiG88", CamoType.Sprite)
      ]),
      rarityClass("KiG88_Epic_Camos", [
        camo("KIG88_POLYCHROMATIC",  1, CamoRarity.Epic, "KiG88_Polychromatic",   "KiG88-PolyChromatic",  CamoType.Sprite),
        camo("KIG88_JAPANISESUNSET", 2, CamoRarity.Epic, "KiG88_JapanesseSunset", "KiG88-JpSunset",       CamoType.Sprite),
        camo("KIG88_DOUBLESUN",      3, CamoRarity.Epic, "KiG88_DoubleSun",       "KiG88-DoubleSun",      CamoType.Sprite),
        camo("KIG88_DARKGOLD",       4, CamoRarity.Epic, "KiG88_DarkGold",        "KiG88-DarkGold",       CamoType.Sprite),
        camo("KIG88_DIAMODMARBLES",  5, CamoRarity.Epic, "KiG88_DiamondMarbles",  "KiG88-DiamodMarbles",  CamoType.Sprite),
        camo("KIG88_CRYSTALISED",    6, CamoRarity.Epic, "KiG88_Crystalised",     "KiG88-Crystalised",    CamoType.Sprite),
        camo("KIG88_WAVYDRIP",       7, CamoRarity.Epic, "KiG88_WavyDrip",        "KiG88-WavyDrip",       CamoType.Sprite),
        camo("KIG88_DARKNIGHTSKY",   8, CamoRarity.Epic, "KiG88_DarkNightSky",    "KiG88-DarkNightSky",   CamoType.Sprite),
        camo("KIG88_GOLDENLINE",     9, CamoRarity.Epic, "KiG88_GoldenLine",      "KiG88-GoldenLine",     CamoType.Sprite)
      ])
    ])

    // HOODGUN, STINGERA27, X39XX follow the exact same pattern...
)
```

| Config | JSON (formatted) | DixScript | Reduction |
|--------|-----------------|-----------|-----------|
| `ALL_SMG_CAMOS_CONFIG` | ~350 lines | ~110 lines | **69%** |
| `ALL_SMG_EQ_AND_CAMO_ATTAINMENT_CONFIGS` | ~280 lines | ~90 lines | **68%** |
| `ALL_SMG_UNIQUE_CAMOS_MISSIONS_CONFIG` | ~230 lines | ~75 lines | **67%** |
| **All 3 configs combined** | **~860 lines** | **~275 lines** | **~68%** |

The real payoff isn't just the size reduction — it's that `CamoAvailableInSeason`, `MaterialAddress`, and the `(Clone)` suffix pattern are now in one place. When season 2 ships and every camo needs `CamoAvailableInSeason = "2"`, that's a one-line edit instead of hunting through three 300-line JSON blobs.

---

## What Is DixScript?

**DixScript** is a data interchange format that combines:
- 📦 **Configuration** (like TOML)
- 🔧 **Compile-time functions** (like Jsonnet, but less cryptic)
- 🔒 **Built-in encryption** (AES-256-GCM, not an afterthought)
- 🗜️ **Automatic compression** (gzip/bzip2/lzma)
- 📋 **Type safety** (enums, strong typing when you want it)
- 🎯 **Zero runtime dependencies** (pure Rust, or C# in the original)

**All in one file with a `.mdix` extension.**

---

## Why DixScript Exists (The Problem It Solves)

### The Config File Problem

Modern projects have **config sprawl**:
```
/config
  ├── base.json          # 500 lines
  ├── development.json   # 300 lines (80% duplicated from base)
  ├── production.json    # 400 lines (90% duplicated)
  ├── secrets.env        # Separate encryption
  ├── validation.js      # Separate validation logic
  ├── build.sh           # Compresses everything
  └── deploy.yaml        # References all of the above
```

**You change one setting** → Update 3 files → Run 2 scripts → Hope you didn't break something.

### The DixScript Solution

```dixscript
@DLM(DCompressor.gzip, DEncryptor.aes256)

@QUICKFUNCS(
  ~createEnemy<object>(name, health, damage) {
    return {
      name   = name,
      health = health,
      damage = damage,
      armor  = health / 10,    // Formula lives here — ONE place
      xp     = health / 2,
      gold   = health / 4
    }
  }
)

@DATA(
  environment<enum> = Environment.PROD

  enemies::
    createEnemy("Goblin", 50, 10),
    createEnemy("Orc", 100, 20),
    createEnemy("Troll", 200, 40)
    // Change formula above? All enemies update instantly.
)

@SECURITY(
  encryption -> { mode = "keyfile", algorithm = "aes256-gcm" }
)
```

**One file.** Config, logic, encryption, compression. Done.

---

## Key Features

### 1. Compile-Time Functions (QuickFuncs)
Write logic once, execute at compile time. No runtime overhead.

```dixscript
@QUICKFUNCS(
  ~calculateDamage<int>(base, difficulty<enum>) {
    multiplier = difficulty == Difficulty.HARD ? 2.0 : 1.0
    return Math.round(base * multiplier)
  }
)

@DATA(
  easy_enemy = { damage = calculateDamage(50, Difficulty.EASY) },
  hard_enemy = { damage = calculateDamage(50, Difficulty.HARD) }
)
```

Functions can call other functions.

### 2. Two-Tier Data System

```dixscript
@DATA(
  // Flat properties (single equals)
  app_name = "MyApp"
  version  = "1.0.0"
  port     = 8080

  // Table properties (single colon)
  server: host = "localhost", port = 8080, ssl = true

  // Group arrays (double colon)
  admins:: "alice", "bob", "charlie"
)
```

### 3. Optional Commas

Commas are optional between entries. Use horizontal style with commas or vertical style without — your choice. Zero formatting debt.

### 4. Built-in Encryption & Compression

```dixscript
@DLM(DCompressor.gzip, DEncryptor.aes256)

@SECURITY(
  encryption -> { mode = "password" }
)

@DATA(
  api_key = "super_secret_key"
)
```

Compile: `mdix compile secrets.mdix --password`  
Output: `secrets.mdix.enc` (compressed + encrypted)

### 5. Enums

```dixscript
@ENUMS(
  LogLevel    { DEBUG = 0, INFO = 1, WARN = 2, ERROR = 3 }
  Environment { DEV = 1, STAGING = 2, PROD = 3 }
)

@DATA(
  log_level<enum>    = LogLevel.INFO
  current_env<enum>  = Environment.PROD
)
```

### 6. Strong Types When You Need Them

```dixscript
@DATA(
  // Inferred
  count   = 42
  price   = 19.99
  enabled = true

  // Explicit
  max_users<int>   = 1000
  tax_rate<float>  = 0.15f
  color<hex>       = #FF5733
  avatar           = b:("base64data...")
  email_regex      = r:("^[a-z@.]+$")
  release_date     = 2025-12-31
)
```

---

## Quick Comparison

| Feature | JSON | YAML | TOML | Jsonnet | **DixScript** |
|---------|------|------|------|---------|---------------|
| Deduplication via functions | ❌ | ❌ | ❌ | ✅ | ✅ |
| Built-in encryption | ❌ | ❌ | ❌ | ❌ | ✅ |
| Built-in compression | ❌ | ❌ | ❌ | ❌ | ✅ |
| Enums | ❌ | ❌ | ❌ | ⚠️ | ✅ |
| Optional commas | ❌ | ✅ | ❌ | ❌ | ✅ |
| Compile-time execution | ❌ | ❌ | ❌ | ✅ | ✅ |
| Zero runtime dependencies | ✅ | ❌ | ✅ | ❌ | ✅ |
| Human-readable | ✅ | ✅ | ✅ | ⚠️ | ✅ |

---

## Section Reference

```dixscript
@CONFIG(         // Compiler settings, metadata
  version -> "1.0.0"
)

@IMPORTS(        // Import from other .mdix files
  Utils from "common/utils.mdix"
)

@DLM(            // Data Lifecycle Modules — compression + encryption
  DCompressor.gzip
  DEncryptor.aes256
)

@ENUMS(          // Named constants
  Status { ACTIVE, INACTIVE, PENDING }
)

@QUICKFUNCS(     // Compile-time functions
  ~calculate<int>(x, y) {
    return x + y
  }
)

@DATA(           // Your actual data
  result = calculate(10, 20)
)

@SECURITY(       // Security configuration
  encryption -> { mode = "password" }
)
```

All sections are optional. Use what you need.

---

## Current Status: Rust Port (Work in Progress)

**Original:** Written in C# (.NET 8), fully functional, available at `https://github.com/Mid-D-Man/DixScript`.  
**This Repo:** Rust port for performance and portability.

### Rust Port Progress

| Component | Status | Notes |
|-----------|--------|-------|
| Utilities | ✅ Complete | Logger, keywords, helpers |
| ErrorManager | ✅ Complete | All 10 error types |
| Lexer | ⏳ In Progress | Core lexing done, optimizing |
| Parser | ⏳ In Progress | AST design complete |
| Semantic Analyzer | ⏳ Pending | |
| QuickFuncs Resolver | ⏳ Pending | |
| Binary Serialization | ⏳ Pending | |
| DLM Pipeline | ⏳ Pending | |
| Runtime API | ⏳ Pending | |

### Language Wrapper Status

All wrappers bind to the Rust runtime via FFI. They compile but are **not usable** until the runtime reaches feature parity.

| Package | Language | Status |
|---------|----------|--------|
| `mdix-ffi` + C# NuGet | C# / Unity | ⏳ Pending runtime |
| `mdix-go` | Go | ⏳ Pending runtime |
| `mdix-java` | Java / Kotlin | ⏳ Pending runtime |
| `mdix-python` | Python | ⏳ Pending runtime |
| `mdix-wasm` | JS / Browser | ⏳ Pending runtime |
| `mdix-c` | C / C++ | ⏳ Pending runtime |

**Why Rust?**
- 🚀 **Performance:** The C# prototype is fast. Rust will be faster.
- 🔧 **Portability:** Compile to native binaries, WASM, embedded.
- 🦀 **Safety:** Ownership model catches bugs at compile time.
- 🌐 **FFI:** One Rust core, wrappers for every major language.

---

## When Should You Use DixScript?

**Great for:** Game data configs (weapons, items, enemies, levels), multi-environment server configs, encrypted secrets bundles, any schema where you're copy-pasting structure repeatedly.

**Maybe not for:** Tiny configs under 50 lines with no repetition, simple key-value stores (TOML is simpler), situations requiring maximum existing tooling support.

**The rule of thumb:** If changing one field currently means editing it in more than three places, DixScript will help.

---

## Getting Started (Once Complete)

```bash
# Via cargo (not yet published)
cargo install mdix-cli

# Basic usage
mdix compile config.mdix
mdix compile secrets.mdix --password
mdix validate config.mdix
mdix convert config.json --to mdix
```

```rust
use dixscript::runtime::Dix;

fn main() {
    let data = Dix::load("config.mdix").unwrap();
    let port: i32 = data.get("server.port").unwrap_or(8080);
    println!("Server on port {}", port);
}
```

---

## Contributing

This is an active project. Contributions welcome once the Rust port reaches feature parity.

Ways to help: report bugs in the C# version (helps inform the Rust port), suggest features, improve docs, write tests, port components once the architecture stabilizes.

**Code style:** Follow `rustfmt.toml` in the repo.

---

## Documentation

- Grammar spec: `others/midx.ebnf`
- C# reference implementation: `https://github.com/Mid-D-Man/DixScript`
- CI results: `https://mid-d-man.github.io/DixScript-Rust/`

---

## License

MIT — use it however you want, commercial or personal. See [LICENSE](LICENSE).

---

## Contact

**Creator:** Mid-D-Man  
**GitHub:** `https://github.com/Mid-D-Man/DixScript-Rust`  
**Original (C#):** `https://github.com/Mid-D-Man/DixScript`

Questions? Found a bug? Open an issue.

---

_"Config files shouldn't require a PhD to maintain."_ — The DixScript Philosophy
