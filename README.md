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
**This Repo:** Rust port — feature-complete, API stable, actively tested via CI.

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
| LSP Server | ✅ Complete | Full IDE integration |
| CLI | ✅ Complete | All commands |
| FFI / C | ✅ Complete | C header + 40+ exported functions |

### Language Wrapper Status

All wrappers bind to the Rust runtime via FFI. The core is complete — wrappers are pending packaging and publishing.

| Package | Language | Status |
|---------|----------|--------|
| `mdix-ffi` + C# NuGet | C# / Unity | ⏳ Bindings generated, NuGet packaging pending |
| `mdix-go` | Go | ⏳ C header generated, Go wrapper pending |
| `mdix-java` | Java / Kotlin | ⏳ Pending JNI wrappers |
| `mdix-python` | Python | ⏳ Pending PyO3 wrappers |
| `mdix-wasm` | JS / Browser | ⏳ Pending wasm-bindgen annotations |
| `mdix-c` | C / C++ | ⏳ Header stable, examples pending |

**Why Rust?**
- 🚀 **Performance:** The C# prototype is fast. Rust is faster.
- 🔧 **Portability:** Native binaries, WASM, embedded.
- 🦀 **Safety:** Ownership model catches bugs at compile time.
- 🌐 **FFI:** One Rust core, wrappers for every major language.

---

## Getting Started
```bash
# Build from source (crates.io publish coming soon)
git clone https://github.com/Mid-D-Man/DixScript-Rust
cd DixScript-Rust
cargo build -p mdix-cli --release

# Basic usage
mdix validate config.mdix
mdix compile config.mdix
mdix compile secrets.mdix --password
mdix convert config.json --to mdix
mdix inspect config.mdix --keys
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

---

## When Should You Use DixScript?

**Great for:** Game data configs (weapons, items, enemies, levels), multi-environment server configs, encrypted secrets bundles, any schema where you're copy-pasting structure repeatedly.

**Maybe not for:** Tiny configs under 50 lines with no repetition, simple key-value stores (TOML is simpler), situations requiring maximum existing tooling support.

**The rule of thumb:** If changing one field currently means editing it in more than three places, DixScript will help.

---

## Contributing

Contributions welcome. The Rust port is feature-complete — areas where help is most useful: language wrapper packaging (C#, Go, Python), LSP editor extensions, documentation, and test coverage.

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
