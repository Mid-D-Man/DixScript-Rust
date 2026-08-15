# DixScript: The Swiss Army Knife of Data Formats

**Config, Code, and Crypto in One `.mdix` File**

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/rust-1.85+-orange.svg)](https://www.rust-lang.org/)
[![Crates.io](https://img.shields.io/crates/v/dixscript.svg)](https://crates.io/crates/dixscript)
[![docs.rs](https://img.shields.io/docsrs/dixscript)](https://docs.rs/dixscript)
[![npm](https://img.shields.io/npm/v/%40midmanstudio%2Fmdix.svg)](https://www.npmjs.com/package/@midmanstudio/mdix)
[![PyPI](https://img.shields.io/pypi/v/midmanstudio-mdix.svg)](https://pypi.org/project/midmanstudio-mdix/)
[![NuGet](https://img.shields.io/nuget/v/MidManStudio.Mdix.svg)](https://www.nuget.org/packages/MidManStudio.Mdix)
[![Downloads](https://img.shields.io/crates/d/dixscript.svg)](https://crates.io/crates/dixscript)

## Documentation Site https://dixscript-docs.pages.dev 

> **"I built this because I was tired of copy-pasting the same JSON config blocks 500 times. Turns out other people hate that too."**  
> — Mid-D-Man, Creator

---
<!-- GitAds-Verify: YZXZH8RCNBZ1H2T4AKYE91PNIFVCNMFS -->
## 🎉 `dixscript` v1.0.0 Is Live — And So Is Most of the Ecosystem

**The Rust port is feature-complete, API stable, and now published across five package registries.** These are fresh releases with no real-world mileage yet — install them, use them in your own projects, and file issues for anything you hit. A handful of language wrappers are code-complete but haven't had their publishing pass yet, and the IDE extensions are still in progress.

| Package | Language / Platform | Status |
|---------|---------------------|--------|
| `dixscript` | Rust (core) | ✅ **crates.io** — [`dixscript = "1.0"`](https://crates.io/crates/dixscript) |
| `mdix-cli` | CLI | ✅ **crates.io** — `cargo install mdix-cli` |
| `mdix-lsp` | LSP server | ✅ **crates.io** — `cargo install mdix-lsp` |
| `@midmanstudio/mdix` | Node.js / Browser (WASM) | ✅ **npm** — `npm install @midmanstudio/mdix` |
| `midmanstudio-mdix` | Python | ✅ **PyPI** — `pip install midmanstudio-mdix` |
| `MidManStudio.Mdix` | C# / Unity | ✅ **NuGet** — `dotnet add package MidManStudio.Mdix` |
| `mdix-go` · `mdix-java` · `mdix-lua` · `mdix-php` · `mdix-odin` | Go · Java/Kotlin · Lua · PHP · Odin | 🔨 Code-complete — publishing pass pending |
| `mdix-c` | C / C++ | 🔨 Header + FFI stable — build from source |
| VS Code · VS for Mac · IntelliJ | IDE extensions | ⏳ In progress |

The C# prototype (`https://github.com/Mid-D-Man/DixScript`) remains the reference implementation for the language itself.

---

## GitAds Sponsored
[![Sponsored by GitAds](https://gitads.dev/v1/ad-serve?source=mid-d-man/dixscript-rust@github)](https://gitads.dev/v1/ad-track?source=mid-d-man/dixscript-rust@github)


## The Origin Story (Or: How Scope Creep Turned Into a Format)

**Started as:** A quick hack to make a mobile game's remote config less painful.

**The Problem:** My game had weapons, camos, attainment missions, and shop data spread across Unity Remote Config as massive nested JSON blobs. Adding a single new field to a camo definition meant updating it in dozens of places. One typo and suddenly every weapon in the game was broken at runtime — with no error until the player hit that screen.

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
        }
      ]
    }
  ]
}
```

This is just the **camo config** for one weapon class. Three separate blobs, hundreds of lines, all duplicating the same structure.

**The Solution:** "What if the shape of a camo was defined once, and I just filled in the data?"

**The Result:** The same config in DixScript:
```dixscript
@ENUMS(
  WeaponClass { BASIC_SMG, RUNIC_SMG, LEGENDARY_SMG }
  CamoRarity  { Basic, Rare, Epic, Legendary, Runic }
  CamoType    { Sprite, SpriteAndMaterial }
)

@QUICKFUNCS(
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
        camo("ALIYAHOO419_HORIZON", 0, CamoRarity.Rare, "Aliyahoo419_Horizon", "Aliyahoo419-Horizon", CamoType.Sprite),
        camo("ALIYAHOO419_ROSE",    1, CamoRarity.Epic, "Aliyahoo419_Rose",    "Aliyahoo419-Rose",    CamoType.Sprite)
      ])
    ])
)
```

| Config | JSON (formatted) | DixScript | Reduction |
|--------|-----------------|-----------|-----------|
| `ALL_SMG_CAMOS_CONFIG` | ~350 lines | ~110 lines | **69%** |
| `ALL_SMG_EQ_AND_CAMO_ATTAINMENT_CONFIGS` | ~280 lines | ~90 lines | **68%** |
| `ALL_SMG_UNIQUE_CAMOS_MISSIONS_CONFIG` | ~230 lines | ~75 lines | **67%** |
| **All 3 configs combined** | **~860 lines** | **~275 lines** | **~68%** |

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
/config
├── base.json          # 500 lines
├── development.json   # 300 lines (80% duplicated from base)
├── production.json    # 400 lines (90% duplicated)
├── secrets.env        # Separate encryption
├── validation.js      # Separate validation logic
├── build.sh           # Compresses everything
└── deploy.yaml        # References all of the above
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
      armor  = health / 10,
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

Commas are optional between entries. Use horizontal style with commas or vertical style without — your choice.

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

## Current Status: Rust Port

**Original:** Written in C# (.NET 8), fully functional, available at `https://github.com/Mid-D-Man/DixScript`.  
**This Repo:** Rust port — feature-complete, API stable, actively tested via CI, and now published to crates.io, npm, PyPI, and NuGet.

### Rust Port Progress

| Component | Status | Notes |
|-----------|--------|-------|
| Utilities | ✅ Complete | Logger, keywords, helpers |
| ErrorManager | ✅ Complete | All 10 error types |
| Lexer | ✅ Complete | All token types |
| Parser | ✅ Complete | All 6 sections |
| Semantic Analyzer | ✅ Complete | All 8 analysis phases |
| QuickFuncs Resolver | ✅ Complete | Full function interpreter |
| Binary Serialization | ✅ Complete | Packer + Unpacker |
| DLM Pipeline | ✅ Complete | Forward and reverse |
| Runtime API | ✅ Complete | Load, access, build, convert |
| LSP Server | ✅ Complete | Full IDE integration — published, editor extensions pending |
| CLI | ✅ Complete | All commands — published |
| FFI / C | ✅ Complete | C header + 40+ exported functions |

### Language Wrapper Status

All wrappers bind to the Rust runtime via FFI. See the publish table near the top of this README for the exact package/registry per language — short version: **Node/WASM, Python, C#, the CLI, and the LSP are published**; **Go, Java, Lua, PHP, and Odin are code-complete and waiting on a publishing pass**; **C/C++ is header-stable FFI**, build from source until it gets one too.

**Why Rust?**
- 🚀 **Performance:** The C# prototype is fast. Rust is faster.
- 🔧 **Portability:** Native binaries, WASM, embedded.
- 🦀 **Safety:** Ownership model catches bugs at compile time.
- 🌐 **FFI:** One Rust core, wrappers for every major language.

---

## Getting Started

### Rust
```bash
cargo add dixscript
```
or in `Cargo.toml`:
```toml
[dependencies]
dixscript = "1.0"
```
```rust
use dixscript::Runtime::DixLoader;
use dixscript::Runtime::DixLoadOptions;

fn main() {
    let loader = DixLoader::new();
    let data = loader.load_text("config.mdix", &DixLoadOptions::new()).unwrap();
    let port: i32 = data.get("server.port").unwrap_or(8080);
    println!("Server on port {}", port);
}
```

### CLI (`mdix-cli`)
```bash
cargo install mdix-cli

mdix validate config.mdix
mdix compile config.mdix
mdix compile secrets.mdix --password
mdix convert config.json --to mdix
mdix inspect config.mdix --keys
```

### Node.js / Browser (`@midmanstudio/mdix`)
```bash
npm install @midmanstudio/mdix
```
WASM-backed, works in both Node and the browser via bundlers. See the [npm package page](https://www.npmjs.com/package/@midmanstudio/mdix) for the current API surface.

### Python (`midmanstudio-mdix`)
```bash
pip install midmanstudio-mdix
```

### C# / Unity (`MidManStudio.Mdix`)
```bash
dotnet add package MidManStudio.Mdix
```

### LSP (`mdix-lsp`)
```bash
cargo install mdix-lsp
```
Full IDE integration (diagnostics, completion, hover) is implemented — wrap it as a VS Code / VS for Mac / IntelliJ extension yourself for now, or wait for the official ones.

---

## When Should You Use DixScript?

**Great for:** Game data configs (weapons, items, enemies, levels), multi-environment server configs, encrypted secrets bundles, any schema where you're copy-pasting structure repeatedly.

**Maybe not for:** Tiny configs under 50 lines with no repetition, simple key-value stores (TOML is simpler), situations requiring maximum existing tooling support.

**The rule of thumb:** If changing one field currently means editing it in more than three places, DixScript will help.

---

## Contributing

Contributions welcome. The Rust port and core wrappers are done — areas where help is most useful: publishing the code-complete wrappers (Go, Java, Lua, PHP, Odin), IDE extension packaging (VS Code, VS for Mac, IntelliJ), documentation, and test coverage.

**Code style:** Follow `rustfmt.toml` in the repo.

---

## Documentation

- API docs: `https://docs.rs/dixscript`
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
