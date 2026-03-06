# DixScript: The Swiss Army Knife of Data Formats

**Config, Code, and Crypto in One `.mdix` File**

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/rust-1.70+-orange.svg)](https://www.rust-lang.org/)
[![Status](https://img.shields.io/badge/status-in_development-blue.svg)]()

> **"I built this because I was tired of copy-pasting the same JSON config blocks 500 times. Turns out other people hate that too."**  
> — Mid-D-Man, Creator

---

## The Origin Story (Or: How Scope Creep Turned Into a Format)

**Started as:** A quick hack to make my Unity game configs less painful.

**The Problem:** My game had dozens of enemy types, weapons, items, and abilities. The JSON files were **massive** and **stupidly repetitive**:

```json
{
  "enemies": [
    {"name": "Goblin", "health": 50, "damage": 10, "armor": 5, "xp": 25, "gold": 12},
    {"name": "Orc", "health": 100, "damage": 20, "armor": 10, "xp": 50, "gold": 25},
    {"name": "Troll", "health": 200, "damage": 40, "armor": 20, "xp": 100, "gold": 50}
    // ... and 47 more enemies with the same formula ...
  ]
}
```

Every time I wanted to tweak the XP formula (`health / 2`) or armor calculation (`health / 10`), I had to update **50 different places**. One typo and suddenly goblins were dropping 10,000 gold.

**The Solution:** "What if I could just write the formula once?"

**What Happened Next:** Classic developer move—instead of using an existing tool (YAML, TOML, Jsonnet, whatever), I built my own format. Then I added features. Then more features. Then I cut features. Then I rewrote the syntax three times. Scope creep happened. Hard.

**Four Months Later:** DixScript v1.0.0 exists, and my 800-line JSON config is now 240 lines. **70% smaller.**

**Then I Realized:** If this scratches my itch, maybe it scratches yours too. So here we are.

---

## What Is DixScript?

**DixScript** is a data interchange format that combines:
- 📦 **Configuration** (like TOML)
- 🔧 **Compile-time functions** (like Jsonnet, but less cryptic)
- 🔒 **Built-in encryption** (AES-256-GCM, not an afterthought)
- 🗜️ **Automatic compression** (gzip/bzip2/lzma)
- 📋 **Type safety** (enums, strong typing when you want it)
- 🎯 **Zero dependencies** (pure Rust, or C# in the original)

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
      name = name,
      health = health,
      damage = damage,
      armor = health / 10,    // Formula lives here, ONE place
      xp = health / 2,
      gold = health / 4
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

## Deduplication: The Real Star

Here's the thing: **DixScript won't magically shrink every file by 70%.**

But if your data is repetitive (and honestly, whose config files *aren't*?), you'll see dramatic size reductions:

### Real-World Results

| File Type | Original Size | After DixScript | Reduction |
|-----------|--------------|-----------------|-----------|
| Game enemy data (JSON) | 800 lines | 240 lines | **70%** |
| API endpoints config | 450 lines | 295 lines | **34%** |
| Multi-env server config | 650 lines | 438 lines | **33%** |
| Simple app config | 120 lines | 105 lines | **12%** |

**The Pattern:** The more repetitive your data, the better DixScript works.

### Why It Works

**Traditional formats force you to repeat structure:**
```json
{
  "server1": {"host": "10.0.0.1", "port": 8080, "ssl": true, "timeout": 5000},
  "server2": {"host": "10.0.0.2", "port": 8080, "ssl": true, "timeout": 5000},
  "server3": {"host": "10.0.0.3", "port": 8080, "ssl": true, "timeout": 5000}
}
```

**DixScript:** Write the structure once, populate with data:
```dixscript
@QUICKFUNCS(
  ~server<object>(ip) {
    return { host = ip, port = 8080, ssl = true, timeout = 5000 }
  }
)

@DATA(
  servers:: server("10.0.0.1"), server("10.0.0.2"), server("10.0.0.3")
)
```

**Want to change `timeout` to 10000?** One edit. Three servers updated.

---

## Key Features (What Makes It Special)

### 1. **Compile-Time Functions (QuickFuncs)**
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

**Functions can call other functions!** (New in v1.0.0)

### 2. **Two-Tier Data System**
Inspired by TOML, but better.

**Flat properties** (simple key-value):
```dixscript
@DATA(
  app_name = "MyApp",
  version = "1.0.0",
  port = 8080
)
```

**Grouped data** (nested structures):
```dixscript
@DATA(
  // Table properties (single colon)
  server.config: host = "localhost", port = 8080, ssl = true
  
  // Group arrays (double colon)
  admins:: "alice", "bob", "charlie"
  
  // Mix and match
  database.primary: host = "db.local", port = 5432
  database.replicas::
    { host = "replica-1", readonly = true },
    { host = "replica-2", readonly = true }
)
```

### 3. **Optional Commas (Formatting Freedom)**
Commas are optional **between entries** (but required in arrays/objects).

```dixscript
@DATA(
  // These are all valid:
  x = 1, y = 2, z = 3          // Horizontal (commas)
  
  x = 1
  y = 2
  z = 3                        // Vertical (no commas) choose your style
  
  server: host = "localhost", port = 8080    // Inline table (commas)
)
```

**Result:** Zero formatting debt. No more "missing comma on line 457" merge conflicts.

### 4. **Built-in Encryption & Compression**
Not a plugin. Not a separate tool. **Part of the format.**

```dixscript
@DLM(
  DCompressor.gzip,      // Compress first
  DEncryptor.aes256      // Then encrypt
)

@SECURITY(
  encryption -> { mode = "password" }  // Or "keyfile"
)

@DATA(
  api_key = "super_secret_key",
  database_password = "another_secret"
)
```

**Compile:** `dixscript compile secrets.mdix --password`  
**Output:** `secrets.mdix.enc` (compressed + encrypted)

### 5. **Enums (Named Constants)**
Because magic numbers are the devil.

```dixscript
@ENUMS(
  LogLevel { DEBUG = 0, INFO = 1, WARN = 2, ERROR = 3 }
  Environment { DEV = 1, STAGING = 2, PROD = 3 }
)

@DATA(
  log_level<enum> = LogLevel.INFO,
  current_env<enum> = Environment.PROD
)
```

### 6. **Imports (Reusable Configs)**
Share common configs across files.

```dixscript
@IMPORTS(
  SharedEnums from "common/enums.mdix",
  HelperFuncs from "utils/helpers.mdix"
)

@DATA(
  status<enum> = SharedEnums.Status.ACTIVE,
  computed = HelperFuncs.calculate(10, 20)
)
```

### 7. **Type System (Strong When You Need It)**
Explicit types when you want them, inferred when you don't.

```dixscript
@DATA(
  // Inferred
  count = 42,              // <int>
  price = 19.99,           // <double>
  enabled = true,          // <bool>
  
  // Explicit
  max_users<int> = 1000,
  tax_rate<float> = 0.15f,
  color<hex> = #FF5733,
  
  // Special types
  avatar = b:("base64data..."),      // Blob
  email_regex = r:("^[a-z@.]+$"),    // Regex
  release_date = 2025-12-31,          // Date
  created_at = 2025-01-15T10:30:00Z   // Timestamp
)
```

---

## Quick Comparison: DixScript vs. The Competition

| Feature | JSON | YAML | TOML | Jsonnet | CUE | **DixScript** |
|---------|------|------|------|---------|-----|---------------|
| Deduplication via functions | ❌ | ❌ | ❌ | ✅ | ✅ | ✅ |
| Built-in encryption | ❌ | ❌ | ❌ | ❌ | ❌ | ✅ |
| Built-in compression | ❌ | ❌ | ❌ | ❌ | ❌ | ✅ |
| Enums | ❌ | ❌ | ❌ | ⚠️ | ✅ | ✅ |
| Type inference | ⚠️ | ⚠️ | ⚠️ | ✅ | ✅ | ✅ |
| Comments | ❌ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Human-readable | ✅ | ✅ | ✅ | ⚠️ | ⚠️ | ✅ |
| Optional commas | ❌ | ✅ | ❌ | ❌ | ✅ | ✅ |
| Compile-time execution | ❌ | ❌ | ❌ | ✅ | ✅ | ✅ |
| Zero dependencies | ✅ | ❌ | ✅ | ❌ | ❌ | ✅ |
| Merge conflict friendly | ❌ | ⚠️ | ✅ | ⚠️ | ⚠️ | ✅ |

**Positioning:** For teams tired of config sprawl—ship secure, deduplicated data bundles that just work.

---

## Real-World Examples

### Example 1: Game Enemy Data

**Before (JSON - 800 lines):**
```json
{
  "enemies": [
    {
      "name": "Goblin",
      "health": 50,
      "damage": 10,
      "armor": 5,
      "xp": 25,
      "gold": 12,
      "loot_table": ["common_sword", "health_potion"],
      "ai_type": "aggressive",
      "spawn_rate": 0.3
    },
    // ... 49 more enemies with similar structure ...
  ]
}
```

**After (DixScript - 240 lines, 70% reduction):**
```dixscript
@ENUMS(
  AIType { PASSIVE, NEUTRAL, AGGRESSIVE, BOSS }
)

@QUICKFUNCS(
  ~createEnemy<object>(name, health, damage, ai<enum>) {
    return {
      name = name,
      health = health,
      damage = damage,
      armor = health / 10,           // Computed
      xp = health / 2,               // Computed
      gold = Math.round(health / 4), // Computed
      loot_table = ["common_sword", "health_potion"],
      ai_type = ai,
      spawn_rate = ai == AIType.BOSS ? 0.01 : 0.3
    }
  }
)

@DATA(
  enemies::
    createEnemy("Goblin", 50, 10, AIType.AGGRESSIVE),
    createEnemy("Orc", 100, 20, AIType.AGGRESSIVE),
    createEnemy("Troll", 200, 40, AIType.AGGRESSIVE),
    createEnemy("Dragon", 1000, 150, AIType.BOSS)
    // ... 46 more, all using the same formula
)
```

**Change XP formula?** One line. 50 enemies updated.

### Example 2: Multi-Environment Server Config

**Before (650 lines across 3 files):**
```
config/
  ├── base.json          (200 lines - common settings)
  ├── development.json   (225 lines - 80% duplicated)
  └── production.json    (225 lines - 80% duplicated)
```

**After (438 lines, single file, 33% reduction):**
```dixscript
@ENUMS(
  Environment { DEV = 1, STAGING = 2, PROD = 3 }
)

@QUICKFUNCS(
  ~serverConfig<object>(env<enum>, suffix) {
    pool = env == Environment.DEV ? 10 :
           env == Environment.STAGING ? 25 : 50
    return {
      host = $"{suffix}-server.local",
      port = 8080,
      pool_size = pool,
      timeout = 5000,
      ssl = env == Environment.PROD
    }
  }
)

@DATA(
  dev = serverConfig(Environment.DEV, "dev"),
  staging = serverConfig(Environment.STAGING, "staging"),
  prod = serverConfig(Environment.PROD, "prod")
)
```

### Example 3: API Rate Limits

**Before (450 lines):**
```json
{
  "endpoints": [
    {"path": "/api/v2/users", "method": "GET", "rate_limit": 100, "auth": true},
    {"path": "/api/v2/users", "method": "POST", "rate_limit": 50, "auth": true},
    {"path": "/api/v2/products", "method": "GET", "rate_limit": 200, "auth": false},
    // ... 20 more endpoints ...
  ]
}
```

**After (295 lines, 34% reduction):**
```dixscript
@ENUMS(
  HttpMethod { GET = 1, POST = 2, PUT = 3, DELETE = 4 }
)

@QUICKFUNCS(
  ~endpoint<object>(resource, method<enum>, auth) {
    limit = method == HttpMethod.GET ? 200 : 50
    return {
      path = $"/api/v2/{resource}",
      method = method,
      rate_limit = limit,
      auth = auth
    }
  }
)

@DATA(
  api_version = 2
  
  endpoints::
    endpoint("users", HttpMethod.GET, true),
    endpoint("users", HttpMethod.POST, true),
    endpoint("products", HttpMethod.GET, false),
    endpoint("products", HttpMethod.POST, true)
    // ... 16 more, all sharing the same logic
)
```

---

## Section Reference (What Goes Where)

DixScript files are organized into **6 optional sections**:

```dixscript
@CONFIG(         // Compiler settings, metadata
  version -> "1.0.0",
  author -> "YourName"
)

@IMPORTS(        // Import from other .mdix files
  Utils from "common/utils.mdix"
)

@DLM(            // Data Lifecycle Modules
  DCompressor.gzip,
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

@SECURITY(       // Security configuration (auto-generated if missing)
  encryption -> { mode = "password" }
)
```

**All sections are optional.** Use what you need.

---

## Current Status: Rust Port (Work in Progress)

**Original:** Written in C# (.NET 8), fully functional, v1.0.0 released.  
**This Repo:** Rust port for performance and portability.

### Port Progress

| Component | Status | Notes |
|-----------|--------|-------|
| Utilities | ✅ Complete | Logger, keywords, helpers |
| ErrorManager | ✅ Complete | All 10 error types, Result<T,E> |
| Lexer (Tokenizer) | ⏳ In Progress | Core lexing done, optimizing |
| Parser | ⏳ Pending | AST design complete |
| Semantic Analyzer | ⏳ Pending | |
| QuickFuncs Resolver | ⏳ Pending | |
| Binary Serialization | ⏳ Pending | |
| DLM Pipeline | ⏳ Pending | |
| Runtime API | ⏳ Pending | |

**Why Rust?**
- 🚀 **Performance:** C# prototype is fast. Rust will be faster.
- 🔧 **Portability:** Compile to native binaries, WASM, embedded systems.
- 🦀 **Safety:** Ownership model catches bugs at compile time.

**ETA:** Aiming for feature parity by Q2 2025.

---

## Getting Started (Once Complete)

**Installation (coming soon):**
```bash
# Via cargo
cargo install dixscript

# Or download binary from releases
```

**Basic usage:**
```bash
# Compile a .dixscript file
dixscript compile config.dixscript

# With encryption
dixscript compile secrets.dixscript --password

# Validate syntax
dixscript validate config.dixscript

# Convert from JSON
dixscript convert config.json --to dixscript
```

**In your Rust code:**
```rust
use dixscript::runtime::Dix;

fn main() {
    let result = Dix::load("config.dixscript");
    
    match result {
        Ok(data) => {
            let port: i32 = data.get("server.port").unwrap_or(8080);
            println!("Server running on port {}", port);
        }
        Err(e) => eprintln!("Failed to load config: {}", e),
    }
}
```

---

## Documentation

**Full docs coming soon!** For now:
- Check `others/midx.ebnf` for the complete grammar
- See `README.md` (C# version) for detailed feature documentation
- Browse `tests/` for usage examples

---

## When Should You Use DixScript?

### ✅ **Great For:**
- Large config files with repetitive structure (game data, API configs)
- Multi-environment deployments (dev/staging/prod)
- Encrypted secrets management
- Projects where you're copy-pasting similar config blocks
- Situations where you need config + logic in one place

### ⚠️ **Maybe Not For:**
- Tiny config files (< 50 lines) with no repetition
- Simple key-value stores (use TOML, it's simpler)
- When you need maximum tooling support (JSON/YAML have *every* tool imaginable)
- Real-time streaming data (this is for configs, not events)

### 🤔 **Ask Yourself:**
1. Am I copy-pasting similar config blocks repeatedly?
2. Do I have formulas/calculations in my configs?
3. Am I managing configs across multiple environments?
4. Do I need built-in encryption/compression?

**If yes to 2+:** DixScript is for you.

---

## Contributing

**This is an active project!** Contributions welcome once the Rust port reaches feature parity.

**Ways to help:**
1. 🐛 **Report bugs** in the C# version (helps inform Rust port)
2. 💡 **Suggest features** (open an issue)
3. 📖 **Improve docs** (always appreciated)
4. 🧪 **Write tests** (can never have too many)
5. 🦀 **Port components** (once architecture stabilizes)

**Code style:** Follow `rustfmt.toml` in the repo.

---

## License

MIT License - see [LICENSE](LICENSE) for details.

**TLDR:** Use it however you want. Commercial, personal, open-source. Just don't blame me if it breaks something. 😅

---

## Acknowledgments

**Inspired by:**
- **TOML** for the two-tier system (but more flexible)
- **Jsonnet** for compile-time functions (but more readable)
- **HCL** for the syntax style (but less verbose)
- **Rust** for proving that safety + performance is possible

**Special thanks to:**
- The Rust community for excellent tooling
- My game project for being the painful use case that started this
- Coffee, for existing

---

## Contact

**Creator:** Mid-D-Man  
**GitHub:** [https://github.com/Mid-D-Man/DixScript-Rust](https://github.com/Mid-D-Man/DixScript-Rust)  
**Original (C#):** [https://github.com/Mid-D-Man/DixScript](https://github.com/Mid-D-Man/DixScript)

**Questions? Found a bug? Want to chat?** Open an issue!

---

## Final Thoughts

I built this because I was tired of JSON hell. Maybe you are too.

If DixScript saves you from updating 50 config files after a single formula change, it's done its job.

If it doesn't fit your use case, that's totally fine—use whatever works for you. No hard feelings. ✌️

**Happy scripting!** 🚀

---

## Quick Links

- 📖 [Full Documentation](docs/) (coming soon)
- 🔧 [API Reference](docs/api/) (coming soon)
- 📝 [Grammar Spec](others/midx.ebnf)
- 🐛 [Issue Tracker](https://github.com/Mid-D-Man/DixScript-Rust/issues)
- 💬 [Discussions](https://github.com/Mid-D-Man/DixScript-Rust/discussions)

---

_"Config files shouldn't require a PhD to maintain."_ — The DixScript Philosophy
