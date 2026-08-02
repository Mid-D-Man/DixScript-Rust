# mdix-cli

**Command-line toolchain for DixScript (`.mdix`) files.**

[![Crates.io](https://img.shields.io/crates/v/mdix-cli.svg)](https://crates.io/crates/mdix-cli)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

DixScript is a data interchange format with compile-time functions, built-in
AES-256 encryption, and optional compression. This crate is the CLI toolchain:
validate, compile, convert, inspect, and manage key files.

## Installation

```bash
cargo install mdix-cli
```

Or build from source:

```bash
git clone https://github.com/Mid-D-Man/DixScript-Rust
cd DixScript-Rust
cargo build -p mdix-cli --release
# binary at target/release/mdix
```

## Quick start

```bash
# Create a config file from a template
mdix create game.mdix --template advanced

# Validate it
mdix validate game.mdix

# Inspect structure and keys
mdix inspect game.mdix --keys

# Compile (runs full DLM pipeline if @DLM is present)
mdix compile game.mdix -o ./dist

# Convert to JSON for external tools
mdix convert game.mdix --to json

# Format in-place
mdix format game.mdix

# Check formatting without modifying (useful in CI)
mdix format --check game.mdix
```

## Commands

| Command | Description |
|---------|-------------|
| `validate` | Parse and semantic-analyse a `.mdix` file |
| `compile` | Run the full DLM pipeline (compression + encryption) |
| `decrypt` | Decrypt a `.mdix.enc` file |
| `convert` | Convert between `.mdix`, JSON, and TOML |
| `create` | Scaffold a new file from a built-in template |
| `format` | Canonically format a file in-place |
| `compact` | Minify or strip comments |
| `inspect` | Show structure, key list, and metadata |
| `key generate` | Generate a new `.mdix.key` file |
| `key validate` | Validate a key file |
| `key info` | Show key algorithm, length, and mode |
| `config` | Manage CLI preferences |
| `debug-tokens` | Dump the token stream (development) |
| `debug-ast` | Dump the parsed AST (development) |
| `debug-symbols` | Dump the symbol table (development) |

All commands accept `--verbose`, `--quiet`, `--json`, and `--no-color`.
The `--json` flag wraps output in `{ "success": bool, "data": ... }` for
scripting and CI pipelines.

## Global flags

```
--verbose    Show per-stage timing and extra detail
--quiet      Suppress all non-error output
--json       Machine-readable JSON on stdout; errors on stderr
--no-color   Disable ANSI colour codes
```

## Examples

### Validate with strict mode (warnings as errors)

```bash
mdix validate --strict config.mdix
```

### Compile encrypted config and specify output directory

```bash
mdix compile secrets.mdix -o ./dist
# Produces: dist/secrets.mdix.enc  dist/secrets.mdix.key
```

### Decrypt with an explicit key file

```bash
mdix decrypt secrets.mdix.enc --key /vault/secrets.mdix.key
```

### Decrypt with a password (prompted interactively)

```bash
mdix decrypt secrets.mdix.enc --password-prompt
```

### Convert JSON to `.mdix`

```bash
mdix convert data.json --to mdix -o data.mdix
```

### Generate a ChaCha20 key file

```bash
mdix key generate --algorithm chacha20 --output config.mdix.key
```

### JSON output for scripting

```bash
mdix validate --json config.mdix | jq '.data.token_count'
mdix compile  --json config.mdix | jq '.data.generated_files'
mdix inspect  --json config.mdix | jq '.data.key_count'
```

## Templates

| Template | Contents |
|----------|----------|
| `basic` | `@CONFIG` + `@DATA` |
| `advanced` | Enums, QuickFuncs, multi-environment data |
| `security` | `@DLM` + `@SECURITY` with AES-256 |
| `dlm` | Compression + encryption pipeline |

## Config keys (`mdix config`)

| Key | Default | Description |
|-----|---------|-------------|
| `default_output_directory` | `./output` | Output dir when `-o` is omitted |
| `default_indent_size` | `2` | Spaces per indent level |
| `use_tabs` | `false` | Use tabs instead of spaces |
| `color_output` | `true` | ANSI colour output |
| `auto_find_key_files` | `true` | Search for `.mdix.key` next to `.mdix.enc` |
| `key_search_paths` | `` | Extra dirs to search for key files |
| `pretty_print_json` | `true` | Pretty-print `--json` output |
| `show_warnings` | `true` | Include warnings in output |
| `max_error_display` | `50` | Max errors before truncating |

## Links

- [Language reference and format docs](https://github.com/Mid-D-Man/DixScript-Rust)
- [dixscript core library](https://crates.io/crates/dixscript)
- [C# reference implementation](https://github.com/Mid-D-Man/DixScript)

## License

MIT — see [LICENSE](../LICENSE).
