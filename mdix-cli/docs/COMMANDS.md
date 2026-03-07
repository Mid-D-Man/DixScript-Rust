# mdix Command Reference

`mdix` is the DixScript (.mdix) file toolchain.  
All commands accept `--verbose`, `--quiet`, `--json`, and `--no-color` as global flags.

---

## Global Flags

| Flag | Description |
|------|-------------|
| `--verbose` | Show per-stage timing and extra detail |
| `--quiet` | Suppress all non-error output |
| `--json` | Machine-readable JSON output on stdout; errors on stderr |
| `--no-color` | Disable ANSI color codes |

---

## validate

Run the full parse and semantic analysis pipeline without producing output files.
```
mdix validate [--strict] <file>
```

**Arguments**

| Argument | Description |
|----------|-------------|
| `<file>` | Path to a `.mdix` file |
| `--strict` | Treat warnings as errors (exit 1) |

**Exit codes**

| Code | Meaning |
|------|---------|
| `0` | File is valid |
| `1` | Parse or semantic error |
| `2` | File not found |

**Examples**
```bash
mdix validate config.mdix
mdix validate --strict config.mdix
mdix validate --json config.mdix
```

**JSON output fields**
```json
{
  "success": true,
  "data": {
    "file": "config.mdix",
    "token_count": 142,
    "warning_count": 0,
    "warnings": [],
    "elapsed_ms": 3.4
  }
}
```

---

## compile

Run the full compilation pipeline including DLM modules (compression, encryption, auditing).
```
mdix compile [--output <dir>] [--skip-dlm] <file>
```

**Arguments**

| Argument | Description |
|----------|-------------|
| `<file>` | Path to a `.mdix` file |
| `--output`, `-o` | Output directory for generated files (default: same dir as input) |
| `--skip-dlm` | Skip compression/encryption pipeline |

**Generated files**

| File | Produced when |
|------|--------------|
| `<name>.mdix.enc` | `@DLM` contains `DEncryptor` |
| `<name>.mdix.key` | `@DLM` contains `DEncryptor` |
| `<name>.mdix.au` | `@DLM` contains `DAuditor` only |

**Exit codes**: `0` success, `1` compile error, `2` file not found.

**Examples**
```bash
mdix compile config.mdix
mdix compile config.mdix -o ./dist
mdix compile secrets.mdix --skip-dlm
```

**JSON output fields**
```json
{
  "success": true,
  "data": {
    "source_path": "config.mdix",
    "generated_files": ["config.mdix.enc", "config.mdix.key"],
    "original_size": 1024,
    "modules_applied": ["DCompressor", "DEncryptor"],
    "elapsed_ms": 12.1
  }
}
```

---

## decrypt

Decrypt a `.mdix.enc` file back to its compiled binary form.
```
mdix decrypt [--key <path>] [--password] [--output <dir>] <file>
```

**Arguments**

| Argument | Description |
|----------|-------------|
| `<file>` | Path to a `.mdix.enc` file |
| `--key` | Explicit path to the `.mdix.key` file (auto-detected if omitted) |
| `--password` | Prompt for a password instead of using a key file |
| `--output`, `-o` | Output directory (default: same dir as input) |

**Key file auto-detection order**: same directory → paths from `config key_search_paths`.

**Exit codes**: `0` success, `1` decryption error, `2` file not found.

**Examples**
```bash
mdix decrypt secrets.mdix.enc
mdix decrypt secrets.mdix.enc --key /vault/secrets.mdix.key
mdix decrypt secrets.mdix.enc --password
```

---

## convert

Convert between `.mdix` and other formats.
```
mdix convert [--to <format>] [--from <format>] [--pretty] [--output <path>] <file>
```

**Supported formats**

| Format token | Extension | Notes |
|-------------|-----------|-------|
| `dixscript` or `mdix` | `.mdix` | DixScript native |
| `json` | `.json` | Standard JSON |
| `toml` | `.toml` | TOML v1 |

**Arguments**

| Argument | Description |
|----------|-------------|
| `<file>` | Input file path |
| `--to` | Target format (required) |
| `--from` | Source format (auto-detected from extension if omitted) |
| `--output`, `-o` | Output file path (default: input name with new extension) |
| `--pretty` | Pretty-print output (default: true) |

**Exit codes**: `0` success, `1` conversion error, `2` file not found, `4` unsupported format.

**Examples**
```bash
mdix convert config.mdix --to json
mdix convert config.mdix --to json -o ./output/config.json
mdix convert data.json --to dixscript
mdix convert config.toml --to dixscript
mdix convert config.mdix --to toml
```

**JSON output fields**
```json
{
  "success": true,
  "data": {
    "input_path": "config.mdix",
    "output_path": "config.json",
    "input_size": "1.2 KB",
    "output_size": "3.4 KB",
    "size_ratio": "283.3%",
    "elapsed_ms": 5.2
  }
}
```

---

## create

Scaffold a new `.mdix` file from a built-in template.
```
mdix create [--template <name>] [--force] <file>
```

**Templates**

| Name | Description |
|------|-------------|
| `basic` | `@CONFIG` + simple `@DATA` (default) |
| `advanced` | Enums, QuickFuncs, multi-environment data |
| `security` | DEncryptor + `@SECURITY` section |
| `dlm` | Compression + encryption DLM pipeline |

**Arguments**

| Argument | Description |
|----------|-------------|
| `<file>` | Output `.mdix` path |
| `--template` | Template name (default: `basic`) |
| `--force` | Overwrite existing file |

**Exit codes**: `0` success, `1` unknown template or write error, `3` file exists (without `--force`).

**Examples**
```bash
mdix create config.mdix
mdix create game.mdix --template advanced
mdix create secrets.mdix --template security --force
```

---

## format

Format a `.mdix` file in-place (or to a new path).
```
mdix format [--output <path>] [--indent <n>] [--tabs] [--check] <file>
```

**Arguments**

| Argument | Description |
|----------|-------------|
| `<file>` | Path to a `.mdix` file |
| `--output`, `-o` | Write formatted output here instead of overwriting input |
| `--indent` | Spaces per indent level (default: 2) |
| `--tabs` | Use tabs instead of spaces |
| `--check` | Exit 1 if file is not already formatted; do not write |

**Examples**
```bash
mdix format config.mdix
mdix format config.mdix --indent 4
mdix format config.mdix --check       # use in CI
```

---

## compact

Remove whitespace or comments from a `.mdix` file.
```
mdix compact [--mode <mode>] [--ratio] [--output <path>] <file>
```

**Modes**

| Mode | Description |
|------|-------------|
| `compact` | Remove trailing whitespace, collapse blank lines (default) |
| `minify` | Remove all unnecessary whitespace (smallest output) |
| `strip-comments` | Remove `//` and `/* */` comments only |

**Arguments**

| Argument | Description |
|----------|-------------|
| `<file>` | Path to a `.mdix` file |
| `--mode` | Compaction mode (default: `compact`) |
| `--ratio` | Print compression ratio |
| `--output`, `-o` | Output path (default: `<name>.<mode>.mdix`) |

**Exit codes**: `0` success, `3` unknown mode.

**Examples**
```bash
mdix compact config.mdix
mdix compact config.mdix --mode minify -o config.min.mdix
mdix compact config.mdix --mode strip-comments --ratio
```

**JSON output fields**
```json
{
  "success": true,
  "data": {
    "input_path": "config.mdix",
    "output_path": "config.compact.mdix",
    "original_size": 2048,
    "compacted_size": 1536,
    "ratio": 0.25
  }
}
```

---

## inspect

Display the structure and data keys of a `.mdix` file.
```
mdix inspect [--sections] [--keys] <file>
```

**Arguments**

| Argument | Description |
|----------|-------------|
| `<file>` | Path to a `.mdix` file |
| `--sections` | Show section summary only |
| `--keys` | List all data keys with their types |

**Examples**
```bash
mdix inspect config.mdix
mdix inspect config.mdix --keys
mdix inspect config.mdix --sections
mdix inspect --json config.mdix
```

**JSON output fields**
```json
{
  "success": true,
  "data": {
    "file_path": "config.mdix",
    "file_size": 1024,
    "sections": ["@CONFIG", "@ENUMS", "@DATA"],
    "key_count": 14,
    "enum_count": 2,
    "dlm_modules": [],
    "version": "1.0.0",
    "keys": [
      { "path": "app_name", "value_type": "string" },
      { "path": "port", "value_type": "int" }
    ]
  }
}
```

---

## key

Manage `.mdix.key` encryption key files.

### key generate
```
mdix key generate [--output <path>] [--algorithm <algo>] [--password]
```

| Argument | Description |
|----------|-------------|
| `--output` | Output path (default: `output.mdix.key`) |
| `--algorithm` | `aes128`, `aes256` (default), or `chacha20` |
| `--password` | Generate a password-mode key (no raw key bytes stored) |
```bash
mdix key generate --output config.mdix.key
mdix key generate --algorithm chacha20 --output secrets.mdix.key
mdix key generate --password --output vault.mdix.key
```

### key validate
```
mdix key validate <keyfile>
```

Validate the structure and fields of a `.mdix.key` file.  
Exit `0` if valid, `1` if invalid, `2` if not found.

### key info
```
mdix key info [--json] <keyfile>
```

Display algorithm, key length, mode, and creation timestamp.
```bash
mdix key info config.mdix.key
mdix key info --json config.mdix.key
```

---

## config

Manage CLI preferences stored at `~/.dixscript/config.toml`.

### config list
```
mdix config list [--json]
```

Show all configuration keys, their current values, and whether each is a default.

### config get
```
mdix config get <key>
```

Print the value of a single key. Exit `1` if the key is unrecognised.

### config set
```
mdix config set <key> <value>
```

Update a key and persist to disk.

### config reset
```
mdix config reset [<key>]
```

Reset one key (or all keys if omitted) to defaults.

**Available keys**

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `default_output_directory` | string | `./output` | Where generated files go when `-o` is not provided |
| `default_indent_size` | int | `2` | Spaces per indent level |
| `use_tabs` | bool | `false` | Use tabs instead of spaces |
| `color_output` | bool | `true` | Enable colored terminal output |
| `auto_find_key_files` | bool | `true` | Search for `.mdix.key` next to `.mdix.enc` |
| `key_search_paths` | string list | `` | Extra directories to search for key files (comma-separated) |
| `pretty_print_json` | bool | `true` | Pretty-print `--json` output |
| `show_warnings` | bool | `true` | Include warnings in command output |
| `max_error_display` | int | `50` | Maximum errors shown before truncating |

**Examples**
```bash
mdix config list
mdix config get default_indent_size
mdix config set default_indent_size 4
mdix config set key_search_paths /etc/keys,/vault/keys
mdix config reset default_indent_size
mdix config reset
```

---

## Test Results Directory

All CLI integration tests write results to `test_results/` in the workspace root when run via `cargo test`. The directory layout is:
```
test_results/
  validate/
  compile/
  convert/
  compact/
  inspect/
```

Run the full integration suite:
```bash
cargo test -p mdix-cli
```

Run the smoke tests (requires a debug build):
```bash
cargo build -p mdix-cli
./mdix-cli/smoke_test.sh
```
