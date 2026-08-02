# mdix-lsp

**Language server for DixScript (`.mdix`) files.**

[![Crates.io](https://img.shields.io/crates/v/mdix-lsp.svg)](https://crates.io/crates/mdix-lsp)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

DixScript is a data interchange format with compile-time functions, built-in
AES-256 encryption, and optional compression. `mdix-lsp` implements the
[Language Server Protocol](https://microsoft.github.io/language-server-protocol/)
over stdio, giving any LSP-compatible editor real-time diagnostics,
completions, and navigation for `.mdix` files, backed directly by the
`dixscript` compiler pipeline.

## Features

- Diagnostics from the real compiler pipeline (tokenizer → parser →
  semantic analysis → AST enhancer), not a separate re-implementation
- Completions and signature help for QuickFuncs, enums, DLM modules, and
  imported namespaces
- Hover documentation for sections, keywords, types, and built-in functions
- Go-to-definition, find references, rename, document/workspace symbols
- Call hierarchy for QuickFuncs
- Inlay type hints and parameter hints
- Semantic tokens (full-file syntax highlighting driven by the real AST)
- Code actions / quick fixes (insert `@SECURITY`, replace weak `xor`, etc.)
- Code lens for compile, convert to JSON/TOML, minify, and resolve
- Color, date/timestamp, and blob literal providers
- Document formatting, on-type formatting, and folding

## Installation

```bash
cargo install mdix-lsp
```

Or build from source:

```bash
git clone https://github.com/Mid-D-Man/DixScript-Rust
cd DixScript-Rust
cargo build -p mdix-lsp --release
# binary at target/release/mdix-lsp
```

Prebuilt binaries (Linux, macOS x86_64/aarch64, Windows) are attached to
each [GitHub release](https://github.com/Mid-D-Man/DixScript-Rust/releases)
tagged `mdix-lsp-v*`, if you'd rather not build from source or install a
Rust toolchain.

## Editor integration

`mdix-lsp` speaks LSP over stdio — point your editor's LSP client at the
binary path and it works with anything that supports the protocol. This
repo ships two ready-made integrations that already do that wiring:

- **VS Code**: [`mdix-vscode`](../mdix-vscode) — set
  `dixscript.server.path` to the `mdix-lsp` binary (or leave it empty to
  use the bundled one).
- **IntelliJ / Rider / other JetBrains IDEs**: [`mdix-intellij`](../mdix-intellij).

For any other editor, point its generic LSP client config at the
`mdix-lsp` binary with no arguments — it starts, waits on stdin, and
speaks LSP immediately.

## Logging

All tracing goes to stderr — stdout is reserved for the LSP stdio channel.

```bash
RUST_LOG=mdix_lsp=debug mdix-lsp        # verbose logging to stderr
MDIX_LSP_LOG=/tmp/mdix-lsp.log mdix-lsp # also mirror logs to a file
```

## Links

- [Language reference and format docs](https://github.com/Mid-D-Man/DixScript-Rust)
- [dixscript core library](https://crates.io/crates/dixscript)
- [mdix-cli](https://crates.io/crates/mdix-cli) — command-line toolchain for the same files
- [C# reference implementation](https://github.com/Mid-D-Man/DixScript)

## License

MIT — see [LICENSE](../LICENSE).
