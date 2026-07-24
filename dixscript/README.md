# dixscript

**DixScript core runtime for Rust** — load, access, build, and convert `.mdix` files.

[![Crates.io](https://img.shields.io/crates/v/dixscript.svg)](https://crates.io/crates/dixscript)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/rust-1.85+-orange.svg)](https://www.rust-lang.org/)
[![CI](https://github.com/Mid-D-Man/DixScript-Rust/actions/workflows/dixscript-publish.yml/badge.svg)](https://github.com/Mid-D-Man/DixScript-Rust/actions)

DixScript is a data interchange format with compile-time functions,
built-in capabilities for  AES-256 encryption, and optional compression. This crate is
the Rust runtime: it compiles `.mdix` source, resolves all QuickFuncs
at compile time, and exposes a flat dotted-path API for reading the
resulting data at runtime.

> **Format documentation and language reference:**
> [`DixScript-Docs.pages.dev`](https://dixscript-docs.pages.dev) ·
> [`github.com/Mid-D-Man/DixScript-Rust`](https://github.com/Mid-D-Man/DixScript-Rust)
>
> **Module and API index for contributors:** see [`APICATALOG.md`](./APICATALOG.md)

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
| `Runtime::DixSerialize` / `DixDeserialize` | Convert between `DixData` and plain Rust structs directly, no intermediate hashmap |
| `Runtime::SchemaBuilder` | Validate loaded data against an expected shape, collecting every violation instead of stopping at the first |
| `Runtime::DixQuery` | LINQ-style `where_`/`order_by`/`select` chaining over array fields |
| `Runtime::MdixMerger` | AST-level merge of multiple sources — weight-based or strict conflict resolution |
| `Runtime::HotReloadWatcher` | Poll-based file-change watcher for reloading config without a restart |
| `Runtime::DixConverter` | Convert between DixScript, JSON, TOML, and `HashMap<String, DixValue>` — see [`from_dix_data` vs `from_hashmap`](#from_dix_data-vs-from_hashmap) below for which one to reach for |
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

## Struct (de)serialization

Convert between `DixData` and plain Rust structs directly, without an
intermediate `HashMap` round-trip. Implement `DixDeserialize`/
`DixSerialize` once per struct and reuse it everywhere `DixData` shows up.

```rust
use dixscript::Runtime::{
    DixData, DataBuilder, DixDataBuilder,
    DixDeserialize, DixSerialize,
    dix_get, dix_set_str, dix_set_int,
};

struct ServerConfig {
    host: String,
    port: i32,
}

impl DixDeserialize for ServerConfig {
    fn from_dix(data: &DixData, prefix: &str) -> Result<Self, String> {
        Ok(ServerConfig {
            host: dix_get(data, prefix, "host")?,
            port: dix_get(data, prefix, "port")?,
        })
    }
}

impl DixSerialize for ServerConfig {
    fn to_dix(&self, d: &mut DataBuilder, prefix: &str) -> Result<(), String> {
        dix_set_str(d, prefix, "host", &self.host);
        dix_set_int(d, prefix, "port", self.port);
        Ok(())
    }
}

// Deserialize a nested table straight into a struct.
let config: ServerConfig = data.deserialize_at("server")?;

// Serialize a struct back into a fresh DixData.
let rebuilt = DixDataBuilder::new()
    .serialize_at("server", &config)
    .build()?;
```

---

## Schema validation

Validate a loaded `DixData` against an expected shape without stopping at
the first violation — `ValidationReport` collects everything wrong in one
pass, which matters when the input might be a modder-supplied or
hand-edited config: better to report every problem at once than make
someone fix-and-rerun repeatedly.

```rust
use dixscript::Runtime::SchemaBuilder;

let report = data.validate_schema(
    SchemaBuilder::new()
        .require_string("server.host")
        .require_int("server.port")
        .require_bool("server.ssl"),
);

if !report.is_valid() {
    for error in &report.errors {
        eprintln!("{}: expected {}, got {}", error.path, error.expected, error.actual);
    }
}
```

---

## Querying

LINQ-style chaining over an array field's elements — filter, sort, and
project without hand-writing the loop. `query(path)` covers a plain
`Array` literal or a `GroupArray`'s items alike; `query_many(pattern)`
matches across sibling paths that share shape via a wildcarded segment.

```dixscript
@DATA(
  tasks::
    { name = "Fix bug",    priority = 3 },
    { name = "Write docs", priority = 1 },
    { name = "Ship it",    priority = 3 }
)
```
```rust
use dixscript::Runtime::DixValue;

let high_priority = data.query("tasks")
    .expect("tasks should be an array")
    .where_(|v| v.field("priority").and_then(DixValue::as_int) == Some(3))
    .order_by_desc(|v| v.field("priority").and_then(DixValue::as_int).unwrap_or(0));

let names: Vec<Option<&str>> = high_priority
    .select(|v| v.field("name").and_then(DixValue::as_string));
// → ["Fix bug", "Ship it"]
```

---

## Merging

AST-level merge of two or more DixScript sources — combine a base config
with environment overrides, or a shipped default with a player's local
save, without hand-rolling a deep merge over `HashMap`s. Conflicts (the
same key present in more than one source with a different value) are
resolved per the chosen `MdixMergeStrategy`; `MergeConflict` records
exactly what was decided, and why.

```rust
use dixscript::Runtime::merge::{MdixMerger, MdixMergeInput, MdixMergeStrategy};

// File-path convenience — loads, compiles, merges, returns DixData directly.
let data = MdixMerger::new().merge_files(&["base.mdix", "overrides.mdix"])?;

// Explicit per-file weights — higher weight wins on conflict.
let data = MdixMerger::new().merge_files_weighted(&[
    ("base.mdix",      1.0),
    ("overrides.mdix", 0.8),
    ("local.mdix",     0.5),
])?;

// Full control: pre-parsed ASTs, labels for readable conflict reports, and
// a strategy that refuses to silently pick a winner.
let result = MdixMerger::new()
    .with_strategy(MdixMergeStrategy::ThrowOnConflict)
    .merge_all(vec![
        MdixMergeInput::new(ast_base).with_weight(1.0).with_label("base"),
        MdixMergeInput::new(ast_patch).with_weight(0.8).with_label("patch"),
    ]);

for conflict in &result.conflicts {
    println!("{conflict}");  // "[Conflict] 'DATA.server.port' → source[1] ('patch') won"
}
```

`mdix diff` (in `mdix-cli`) is built directly on this — it runs
`ThrowOnConflict` specifically to enumerate every disagreement between
files without picking a winner, then reports `result.conflicts` as-is.

---

## Hot reload

A poll-based file-change watcher for Rust consumers — call
`check_and_reload()` once per game loop tick / server poll cycle; it only
does real work (re-reading and re-compiling the file) when the
modification time has actually changed.

```rust
use dixscript::Runtime::HotReloadWatcher;

let mut watcher = HotReloadWatcher::new("config.mdix");

// in your game loop / tick / update:
match watcher.check_and_reload() {
    Ok(Some(data)) => apply_new_config(data),  // file changed, reloaded
    Ok(None)       => {}                       // unchanged, nothing to do
    Err(e)         => eprintln!("hot reload failed: {e}"),
}
```

Each language binding (WASM/Python/C#/...) implements its own native
filesystem-event mechanism instead (inotify, FSEvents,
ReadDirectoryChangesW) rather than polling — this Rust-only watcher is
the simple, dependency-free default for direct `dixscript` consumers.

---

## Converting formats
```rust
use dixscript::Runtime::DixConverter;

let converter = DixConverter::new();

// Load a .mdix file and export as JSON
let loader = DixLoader::new();
let data   = loader.load_text("config.mdix", &DixLoadOptions::new())?;
let ast    = converter.from_dix_data(&data)?;
let json   = converter.to_json(&ast, true /* pretty */)?;

// Parse JSON and convert to .mdix
let ast2  = converter.from_json(&json)?;
let mdix  = converter.to_mdix(&ast2, None)?;

// Round-trip through TOML
let toml  = converter.to_toml(&ast)?;
let ast3  = converter.from_toml(&toml)?;
```

### `from_dix_data` vs `from_hashmap`

`DixConverter` has two ways to turn loaded data back into a `DixScript`
AST — pick based on what you actually have on hand:

- **`from_dix_data(&data)`** — use this whenever you already have a real
  `DixData` (the common case: anything that came out of `DixLoader`). It
  reads the genuine `@CONFIG` and `@ENUMS` straight from `DixData::config`
  / `DixData::enums`, so the round trip is a faithful reconstruction, not
  a guess.
- **`from_hashmap(map)`** — use this only when all you have is a bare
  `HashMap<String, DixValue>` with no other context — e.g. a map you built
  by hand, or the internals of `from_json`/`from_toml` (JSON and TOML have
  no config/enum concept of their own, so there's nothing extra to pull
  from). It still reconstructs a usable `@ENUMS` section by scanning the
  map for `DixValue::Enum` usage, but the emitted `@CONFIG` is a synthetic
  placeholder (`version = "1.0.0"` only).

Note: JSON and TOML have no native enum type, so `from_json`/`from_toml`
round trips always lose the symbolic enum name — the integer survives,
the `EnumName.FIELD` identity doesn't. That's an inherent limitation of
those formats, not something either `from_*` method can recover.

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
| `Long(i64)` | `i64` | `9_000_000_000L` |
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

```toml
[dependencies]
dixscript = "1.0.0"
```
pulls in everything below by default — existing behavior is unchanged if
you don't touch this. To trim what you don't need:
```toml
dixscript = { version = "1.0.0", default-features = false, features = ["xz-support"] }
```

| Feature | Default | What it adds |
|---------|---------|---------------|
| `cloud-import` | on | HTTP/HTTPS `@IMPORTS` resolution (reqwest + rustls-tls) |
| `bzip2-support` | on | bzip2 compression for `@DLM(DCompressor.bzip2)` |
| `xz-support` | on | XZ/LZMA compression for `@DLM(DCompressor.lzma)` |
| `rayon-support` | on | Parallel section parsing/(de)serialization for large files |

Building with a feature off and then loading a `.mdix` file that actually
needs it (e.g. `xz-support` disabled but the file specifies
`DCompressor.lzma`) returns a clear `Err` naming the missing feature —
never a panic.

### Platform notes

- **gzip, bzip2, and XZ compression** all work identically on every
  target — native, `wasm32-unknown-unknown`, and Android. All three
  backends are pure Rust (bzip2 via `libbz2-rs-sys`, XZ via `lzma-rust2`,
  a real ported encoder, not a "compiles but barely compresses"
  placeholder) — no C toolchain, no NDK cross-compile pain, no wasm build
  failures. This wasn't always true; the platform notes here used to say
  bzip2/lzma were excluded on wasm32 — that was accurate for older
  versions and is no longer accurate as of this release.
- **All encryption algorithms** (AES-128, AES-256-GCM, ChaCha20-Poly1305)
  work on every target including `wasm32` and Android — pure Rust
  RustCrypto primitives throughout, no exceptions.
- **`rayon-support`** parallelizes on native targets when enabled (on by
  default) and always falls back to sequential processing on `wasm32`
  regardless of the feature flag — there's no real thread pool available
  there to parallelize onto in the first place.
- **`cloud-import` does not actually fetch anything on `wasm32`.** There's
  no way to make a real, safe synchronous network request from inside a
  wasm module — the `@IMPORTS` cloud path returns a clear error on that
  target instead of silently failing. The working pattern on wasm is: the
  host (JS) does a normal `fetch()` itself, then seeds a cache the
  synchronous resolver checks first — see `mdix-wasm`'s `prefetchImport()`
  binding. Local (non-cloud) `@IMPORTS` file paths have the same
  limitation on wasm32 for the same underlying reason (no real
  filesystem) — the host is expected to hand fully-assembled source to
  `loadStr()` rather than DixScript resolving imports itself.

---

## MSRV

Rust **1.85** or later. (`bzip2` 0.6's pure-Rust backend needs 1.82;
`lzma-rust2` needs 1.85 — the higher of the two is the real floor.)

---

## License

MIT — see [LICENSE](../LICENSE).

---

## Links

- [Format reference & language spec](https://github.com/Mid-D-Man/DixScript-Rust)
- [C# reference implementation](https://github.com/Mid-D-Man/DixScript)
- [Module & API catalogue](./APICATALOG.md)
- [Changelog](./CHANGELOG.md)
- [CI results & benchmarks](https://mid-d-man.github.io/DixScript-Rust/)
- [mdix-cli](https://crates.io/crates/mdix-cli) — command-line toolchain
