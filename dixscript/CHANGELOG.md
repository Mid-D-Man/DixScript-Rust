# Changelog

All notable changes to DixScript are documented in this file.

Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Versioning follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [1.0.0] — 2026-05-07

First stable release of the Rust port. The C# reference implementation
(`github.com/Mid-D-Man/DixScript`) remains available; this crate is the
production Rust runtime.

### Added — `dixscript` (core library)

- Full compilation pipeline: config → tokenizer → parser → semantic analyzer →
  AST enhancer → value resolver → binary serializer
- All six `.mdix` sections: `@CONFIG`, `@IMPORTS`, `@DLM`, `@ENUMS`,
  `@QUICKFUNCS`, `@DATA`, `@SECURITY`
- Two-tier data ordering enforced at compile time and in `DixDataBuilder`
- Compile-time QuickFuncs: full expression evaluator including arithmetic,
  comparisons, ternary, string interpolation, and nested function calls
- Built-in encryption: AES-128-GCM, AES-256-GCM, ChaCha20-Poly1305
- Built-in compression: gzip (pure Rust), bzip2, lzma (native platforms only)
- DAuditor pipeline module with DIY and Enhanced subtypes
- Binary serialization (packer + unpacker) for compiled `.mdix.enc` files
- Key file format: generated `.mdix.key` files with Argon2id KDF for
  password-mode encryption
- `DixLoader`: load from disk, string, encrypted bytes, or encrypted file
- `DixData`: O(1) flat dotted-path access, wildcard selection, prefix index
- `DixDataBuilder`: fluent builder for runtime save data and config construction
- `DixConverter`: bidirectional conversion between `.mdix`, JSON, and TOML
- `DixCompactor`: minify, compact, and strip-comments operations
- `DixDeserialize` trait: implement once, call `data.deserialize_at("prefix")`
- `DixSerialize` trait: implement once, call `builder.serialize_at("prefix", &val)`
- Helper functions: `dix_get`, `dix_get_or`, `dix_nested`, `dix_array_of`,
  `dix_set_str`, `dix_set_int`, `dix_set_bool`, `dix_set_double`, `dix_set_nested`
- `SchemaBuilder` and `ValidationReport` for runtime schema enforcement
- Full LSP server (`mdix-lsp`) with hover, completion, diagnostics, folding,
  and document symbols
- FFI layer: 40+ exported C functions via `mdix-ffi`
- ErrorManager: 10 error categories, isolated per-loader error state
- `DixFormatOptions`: readable, compact, pretty, and minified serialization modes
- `DixLoadOptions`: password, key-file path, key-file content, URL, and
  search-path key resolution strategies

### Added — `mdix-cli`

- `mdix validate` — full parse + semantic analysis, `--strict` mode
- `mdix compile` — full DLM pipeline with `--output` and `--skip-dlm`
- `mdix decrypt` — reverse pipeline with key-file auto-detection, password
  prompt, and `MDIX_DLM_PASSWORD` environment variable support
- `mdix convert` — `.mdix` ↔ JSON ↔ TOML with `--pretty` and round-trip support
- `mdix create` — template scaffolding: `basic`, `advanced`, `security`, `dlm`
- `mdix format` — canonical formatting with `--indent`, `--tabs`, `--check`
- `mdix compact` — three modes: `compact`, `minify`, `strip-comments`
- `mdix inspect` — structure overview with `--keys` and `--sections` flags
- `mdix key generate` — AES-128, AES-256, ChaCha20, password mode
- `mdix key validate` — structural validation of `.mdix.key` files
- `mdix key info` — algorithm, key length, mode, creation timestamp
- `mdix config list/get/set/reset` — persistent CLI preferences at
  `~/.dixscript/config.toml`
- `mdix debug-tokens` — full token stream dump with section filters
- `mdix debug-ast` — parsed and enhanced AST dump per section
- `mdix debug-symbols` — semantic analysis symbol table dump
- Global flags: `--verbose`, `--quiet`, `--json`, `--no-color`
- Machine-readable JSON envelope on all commands via `--json`
- Integration test suite covering all major commands
- Smoke test script (`smoke_test.sh`) and PowerShell equivalent (`dev.ps1`)

### Language features (DixScript v1.0.0)

- Optional commas between data entries, array items, and table properties
- Kebab-case identifiers in `@DATA` (e.g. `my-weapon-class`)
- Prefixed constructors: `b:(...)` blob, `r:(...)` regex, `t:(...)` tuple
- Interpolated strings: `$"Hello {name}"`
- Date (`2025-12-31`) and timestamp (`2025-12-31T10:30:00Z`) literals
- Hex color literals (`#FF5733`) as a distinct value type
- Hex integer literals (`0xFF`)
- Enum auto-increment when value is omitted
- `global` scope modifier for QuickFuncs
- `chk:` / `miss` switch statement
- `let`, `let mut`, `const` variable declarations in QuickFuncs
- Arithmetic assignment operators: `+=`, `-=`, `*=`, `/=`, `%=`
- Multi-line comments (`/* */`) and single-line comments (`//`)
- Cloud imports (`from_cloud`) with optional `verify` hash
- `@IMPORTS` namespace aliasing
- `Dix.logEvent`, `Dix.getSystemInfo`, `Dix.validateConfig` builtins

### Platform support

| Platform | Status |
|----------|--------|
| Linux x86-64 | ✅ |
| macOS x86-64 | ✅ |
| macOS aarch64 (Apple Silicon) | ✅ |
| Windows x86-64 | ✅ |
| Android (via FFI) | ✅ |
| wasm32 (gzip only, no bzip2/lzma) | ✅ |

### MSRV

Rust **1.70** or later.

---

*Older history lives in the C# reference implementation at
`github.com/Mid-D-Man/DixScript`.*
