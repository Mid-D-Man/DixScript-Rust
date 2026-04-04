# dixscript

**DixScript core runtime for Rust** — load, access, build, and convert `.mdix` files.

[![Crates.io](https://img.shields.io/crates/v/dixscript.svg)](https://crates.io/crates/dixscript)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/rust-1.70+-orange.svg)](https://www.rust-lang.org/)
[![CI](https://github.com/Mid-D-Man/DixScript-Rust/actions/workflows/dixscript-publish.yml/badge.svg)](https://github.com/Mid-D-Man/DixScript-Rust/actions)

DixScript is a data interchange format with compile-time functions,
built-in AES-256 encryption, and optional compression. This crate is
the Rust runtime: it compiles `.mdix` source, resolves all QuickFuncs
at compile time, and exposes a flat dotted-path API for reading the
resulting data at runtime.

> **Format documentation and language reference:**
> [`github.com/Mid-D-Man/DixScript-Rust`](https://github.com/Mid-D-Man/DixScript-Rust)

---

## Quick start
```toml
[dependencies]
dixscript = "1.0.0"
```
```rust
use dixscript::Runtime::{DixLoader, DixLoadOptions};

fn main() {
    let loader = DixLoader::new();
    let data   = loader.load_text("config.mdix", &DixLoadOptions::new()).unwrap();

    let port: i32    = data.get("server.port").unwrap_or(8080);
    let host: String = data.get("server.host").unwrap_or("localhost".into());
    println!("Connecting to {}:{}", host, port);
}
```

`config.mdix`:
@DATA(
server: host = "api.example.com", port = 443, ssl = true
)
---

## What this crate provides

| Module | What it does |
|--------|-------------|
| `Runtime::DixLoader` | Compile and load `.mdix` files from disk, string, or encrypted bytes |
| `Runtime::DixData` | O(1) flat dotted-path access to loaded data |
| `Runtime::DixValue` | Runtime value type — 15 variants covering all DixScript types |
| `Runtime::DixDataBuilder` | Fluent builder for creating save data at runtime without a template |
| `Runtime::DixConverter` | Convert between DixScript, JSON, TOML, and `HashMap<String, DixValue>` |
| `Runtime::DixCompactor` | Minify and compact `.mdix` source text |
| `Runtime::DixLoadOptions` | Configure loading: passwords, key files, output directories |
| `Runtime::DixFormatOptions` | Configure serialization: indentation, minification, section inclusion |
| `Runtime::KeyResolver` | Derive or extract AES/ChaCha20 key bytes for encrypted files |

---

## Loading files

### Plain `.mdix` from disk
```rust
use dixscript::Runtime::{DixLoader, DixLoadOptions};

let loader = DixLoader::new();
let data   = loader.load_text("game/enemies.mdix", &DixLoadOptions::new())?;
```

### From a string — useful for Unity TextAssets or embedded configs
```rust
let source = include_str!("../assets/config.mdix");
let data   = loader.load_from_str(source, &DixLoadOptions::new())?;
```

### Encrypted file with a key file
```rust
use dixscript::Runtime::DixLoadOptions;

let opts = DixLoadOptions::with_key_file("secrets.mdix.key");
let data = loader.load_encrypted("secrets.mdix.enc", &opts)?;
```

### Encrypted file with a password
```rust
let opts = DixLoadOptions::with_password("my_password");
let data = loader.load_encrypted("secrets.mdix.enc", &opts)?;
```

### Encrypted bytes in memory — for platforms without filesystem access
```rust
let encrypted_bytes: &[u8] = /* from network, asset bundle, etc. */;
let key_content: &str      = /* .mdix.key file contents as a string */;

let data = loader.load_from_encrypted_bytes(
    encrypted_bytes,
    key_content,
    &DixLoadOptions::new(),
)?;
```

---

## Reading data

`DixData` stores everything in a flat `HashMap<String, DixValue>` keyed
by dotted paths. Nested structures from the `.mdix` source are flattened
at load time, so all access is O(1).

### Typed getters via `TryFrom`
```rust
// Returns Err if the path does not exist or the type does not match
let port:    i32    = data.get("server.port")?;
let host:    String = data.get("server.host")?;
let enabled: bool   = data.get("feature_flags.dark_mode")?;
let ratio:   f64    = data.get("config.ratio")?;

// Returns a default instead of Err
let timeout: i32 = data.get_or_default("server.timeout", 30);
```

### Raw value access
```rust
use dixscript::Runtime::DixValue;

match data.get_value("enemy.ai_type") {
    Some(DixValue::Enum { enum_name, field_name, value }) => {
        println!("{}.{} = {}", enum_name, field_name, value);
    }
    Some(other) => println!("unexpected type: {}", other.type_name()),
    None        => println!("path not found"),
}
```

### Checking existence
```rust
if data.exists("optional.feature") {
    let val: String = data.get("optional.feature")?;
}
```

### Array access
```dixscript
@DATA(
  tags:: "alpha", "beta", "v1"
)
```
```rust
// The array itself
let tags: Vec<DixValue> = data.get("tags")?;

// Individual indexed elements
let first: String = data.get("tags[0]")?;
let second: String = data.get("tags[1]")?;
```

### Wildcard selection
```rust
// Matches tags.0.name, tags.1.name, tags.2.name, ...
let names: Vec<String> = data.select_many("tags.*.name");
```

### Key navigation
```rust
// Top-level keys
let top: Vec<String> = data.get_keys("");

// Children of a prefix
let server_keys: Vec<String> = data.get_keys("server");
// → ["host", "port", "ssl"]
```

### Metadata
```rust
println!("version:     {}", data.version);
println!("encrypted:   {}", data.is_encrypted);
println!("compressed:  {}", data.is_compressed);

if let Some(cfg) = &data.config {
    println!("author: {}", cfg.get("author").unwrap_or(&"unknown".to_string()));
}
```

---

## Enums

DixScript enums are resolved at compile time. At runtime you get the
enum name, field name, and integer value — no string parsing required.
```dixscript
@ENUMS(
  AIType { PASSIVE = 0, AGGRESSIVE = 1, BOSS = 2 }
)

@DATA(
  enemy_type<enum> = AIType.BOSS
)
```
```rust
use dixscript::Runtime::DixValue;

match data.get_value("enemy_type") {
    Some(DixValue::Enum { enum_name, field_name, value }) => {
        // enum_name  = "AIType"
        // field_name = "BOSS"
        // value      = 2
        assert_eq!(*value, 2i32);
    }
    _ => {}
}

// Or just get the integer
let ai_type: i32 = data.get("enemy_type")?;  // → 2
```

---

## Building data at runtime

`DixDataBuilder` lets you construct save data, user preferences, or
any runtime-generated config without needing a template `.mdix` file.
```rust
use dixscript::Runtime::DixDataBuilder;

let data = DixDataBuilder::new()
    .config(|c| {
        c.with_version("1.0.0");
        c.with_author("MyGame");
    })
    .enums(|e| {
        e.with_enum_values("Difficulty", &[
            ("EASY",   0),
            ("NORMAL", 1),
            ("HARD",   2),
        ]);
    })
    .data(|d| {
        // Flat properties first
        d.with_string("player_name", "Alice");
        d.with_int("level", 12);
        d.with_bool("tutorial_complete", true);

        // Then grouped data
        d.with_table_properties("settings", |t| {
            t.with_bool("music", true);
            t.with_int("volume", 80);
        });

        d.with_group_array_builder("unlocked_levels", |arr| {
            arr.add_int(1);
            arr.add_int(2);
            arr.add_int(3);
        });
    })
    .build()?;

// Read it back the same way as loaded data
let name: String = data.get("player_name")?;
let vol:  i32    = data.get("settings.volume")?;
```

**Two-tier ordering rule:** flat properties (`with_string`, `with_int`,
etc.) must be added before any table properties or group arrays. Adding
a flat property after grouped data returns `Err` from `build()` with a
descriptive message — it does not panic.

---

## Converting formats
```rust
use dixscript::Runtime::DixConverter;

let converter = DixConverter::new();

// Load a .mdix file and export as JSON
let loader = DixLoader::new();
let data   = loader.load_text("config.mdix", &DixLoadOptions::new())?;
let map    = data.to_hashmap();
let ast    = converter.from_hashmap(map)?;
let json   = converter.to_json(&ast, true /* pretty */)?;

// Parse JSON and convert to .mdix
let ast2  = converter.from_json(&json)?;
let mdix  = converter.to_mdix(&ast2, None)?;

// Round-trip through TOML
let toml  = converter.to_toml(&ast)?;
let ast3  = converter.from_toml(&toml)?;
```

---

## Compacting and minifying source
```rust
use dixscript::Runtime::DixCompactor;

let source = std::fs::read_to_string("config.mdix")?;

// Remove all unnecessary whitespace — smallest output
let minified = DixCompactor::minify(&source);

// Remove trailing whitespace and collapse blank lines — keeps readability
let compacted = DixCompactor::compact(&source);

// Strip comments only
let no_comments = DixCompactor::remove_comments(&source);

// How much smaller?
let ratio = DixCompactor::get_compression_ratio(&source, &minified);
println!("Reduced by {:.1}%", ratio * 100.0);
```

---

## Format options
```rust
use dixscript::Runtime::{DixConverter, DixFormatOptions};

let converter = DixConverter::new();
let ast       = /* ... */;

// Default: indented, 2-space, with @CONFIG section
let readable = converter.to_mdix(&ast, None)?;

// Pretty: 4-space, sorted keys, with type annotations
let verbose = converter.to_mdix(&ast, Some(&DixFormatOptions::pretty()))?;

// Compact: no indentation, no comments, no @CONFIG
let small = converter.to_mdix(&ast, Some(&DixFormatOptions::compact()))?;

// Minified: single line, no whitespace
let tiny = converter.to_mdix(&ast, Some(&DixFormatOptions::minified()))?;

// Custom
let mut opts = DixFormatOptions::new();
opts.indent_size = 4;
opts.use_tabs    = false;
opts.sort_keys   = true;
let custom = converter.to_mdix(&ast, Some(&opts))?;
```

---

## Load options reference
```rust
use dixscript::Runtime::DixLoadOptions;

// Default — no encryption, validates checksums
let opts = DixLoadOptions::new();

// Password decryption
let opts = DixLoadOptions::with_password("my_password");

// Explicit key file path
let opts = DixLoadOptions::with_key_file("/secure/vault/config.mdix.key");

// Key file content from a secrets manager (e.g. HashiCorp Vault)
let opts = DixLoadOptions::with_key_content(
    key_file_string,
    true, // acknowledge_security_risk — required
)?;

// HTTPS URL key loading (trusted internal service only)
let opts = DixLoadOptions::with_key_url(
    "https://internal.vault/keys/config.mdix.key",
    true, // acknowledge_security_risk — required
)?;

// Custom output directory for generated .enc/.key files
let opts = DixLoadOptions::with_output_directory("./dist");

// Additional directories to search for key files automatically
let opts = DixLoadOptions::with_key_search_paths(vec![
    "/etc/myapp/keys".to_string(),
    "/vault/keys".to_string(),
]);
```

---

## Use cases

### Game configuration (Unity / Bevy / Godot)

Define weapon stats, enemy AI types, shop items, and camo configs with
a single function call. `CamoAvailableInSeason` across 60 weapons is
one line. QuickFuncs eliminate all structural boilerplate at compile
time — the binary contains only resolved data.
```dixscript
@QUICKFUNCS(
  ~weapon<object>(id, class<enum>, baseDamage<int>) {
    return {
      id          = id,
      class       = class,
      damage      = baseDamage,
      critChance  = 0.15f,
      range       = baseDamage * 2
    }
  }
)

@DATA(
  weapons::
    weapon("AK47",   WeaponClass.ASSAULT, 35),
    weapon("SHOTGUN", WeaponClass.HEAVY,  80),
    weapon("PISTOL",  WeaponClass.SIDEARM, 18)
)
```
```rust
let damage: i32 = data.get("weapons[1].damage")?;   // 80
let range:  i32 = data.get("weapons[1].range")?;    // 160
```

### Multi-environment server config
```dixscript
@ENUMS(
  Env { DEV = 1, STAGING = 2, PROD = 3 }
)

@QUICKFUNCS(
  ~db<object>(host, port<int>, ssl<bool>) {
    return { host = host, port = port, ssl = ssl }
  }
)

@DATA(
  current_env<enum> = Env.PROD

  database: db("db.prod.internal", 5432, true)
  cache:    host = "redis.prod.internal", port = 6379
)
```
```rust
let host: String = data.get("database.host")?;
let ssl:  bool   = data.get("database.ssl")?;
```

### Encrypted secrets bundle
```dixscript
@DLM(DCompressor.gzip, DEncryptor.aes256)

@DATA(
  stripe_secret = "sk_live_..."
  jwt_secret    = "hs512_..."
  db_password   = "..."
)

@SECURITY(
  encryption -> { mode = "keyfile", algorithm = "aes256-gcm" }
)
```
```bash
mdix compile secrets.mdix --output ./dist
# Produces: dist/secrets.mdix.enc + dist/secrets.mdix.key
```
```rust
let opts   = DixLoadOptions::with_key_file("dist/secrets.mdix.key");
let data   = loader.load_encrypted("dist/secrets.mdix.enc", &opts)?;
let secret: String = data.get("stripe_secret")?;
```

### Runtime save data (games, apps)
```rust
use dixscript::Runtime::DixDataBuilder;

// Build player save data
let save = DixDataBuilder::new()
    .data(|d| {
        d.with_string("player.name", "Alice");
        d.with_int("player.level", 12);
        d.with_int("player.xp", 4800);
        d.with_table_properties("player.position", |t| {
            t.with_double("x", 128.5);
            t.with_double("y", 0.0);
            t.with_double("z", -64.3);
        });
    })
    .build()?;

// Reload the same save
let x: f64 = save.get("player.position.x")?;
```

---

## DixValue variants

| Variant | Rust type | DixScript literal |
|---------|-----------|------------------|
| `Null` | — | `null` |
| `Bool(bool)` | `bool` | `true` / `false` |
| `Int(i32)` | `i32` | `42` |
| `Float(f32)` | `f32` | `3.14f` |
| `Double(f64)` | `f64` | `3.14159` |
| `String(String)` | `String` | `"hello"` |
| `Date(String)` | `String` | `2025-12-31` |
| `Timestamp(String)` | `String` | `2025-12-31T10:30:00Z` |
| `HexColor(String)` | `String` | `#FF5733` |
| `Blob(String)` | base64 `String` | `b:("...")` |
| `Regex(String)` | pattern `String` | `r:("^[a-z]+$")` |
| `Array(Vec<DixValue>)` | `Vec<DixValue>` | `:: a, b, c` |
| `Object(HashMap<String, DixValue>)` | `HashMap` | `{ x = 1, y = 2 }` |
| `Tuple(Vec<DixValue>)` | `Vec<DixValue>` | `t:(1, "a", true)` |
| `Enum { enum_name, field_name, value }` | `i32` via `TryFrom` | `MyEnum.VALUE` |

---

## Error handling

Every public function that can fail returns `Result<T, String>`.
The error string describes what went wrong and where — path not found,
type mismatch, parse failure, decryption error, and so on.
```rust
match loader.load_text("config.mdix", &DixLoadOptions::new()) {
    Ok(data)  => { /* use data */ }
    Err(msg)  => eprintln!("Load failed: {}", msg),
}

match data.get::<i32>("server.port") {
    Ok(port)  => println!("port: {}", port),
    Err(msg)  => eprintln!("Read failed: {}", msg),
}
```

`DixDataBuilder::build()` collects **all** violations before returning
`Err` so you see every problem at once rather than fixing them one at a
time.

---

## Feature flags

No optional features are required for the core use case. The crate
compiles to a pure-Rust library with no system dependencies.

Platform notes:
- `bzip2` and `lzma` compression are excluded on `wasm32` targets
  (gzip remains available via the pure-Rust backend)
- `rayon` parallel parsing is excluded on `wasm32`
- All encryption algorithms (AES-128, AES-256, ChaCha20) work on all
  targets including `wasm32` and Android

---

## MSRV

Rust **1.70** or later.

---

## License

MIT — see [LICENSE](../LICENSE).

---

## Links

- [Format reference & language spec](https://github.com/Mid-D-Man/DixScript-Rust)
- [C# reference implementation](https://github.com/Mid-D-Man/DixScript)
- [CI results & benchmarks](https://mid-d-man.github.io/DixScript-Rust/)
- [mdix-cli](https://crates.io/crates/mdix-cli) — command-line toolchain
