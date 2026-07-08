# DixScript (.mdix) for VS Code

Full IDE support for DixScript `.mdix` files, powered by `mdix-lsp`.

## Features

- Syntax highlighting for all DixScript sections (`@CONFIG`, `@IMPORTS`, `@DLM`, `@ENUMS`, `@QUICKFUNCS`, `@DATA`, `@SECURITY`)
- Hover documentation for sections, keywords, types, and built-in functions
- Completions and signature help for QuickFuncs, enums, and imported namespaces
- Go-to-definition, find references, and rename
- Real-time diagnostics from the DixScript compiler pipeline
- Inlay type hints and parameter hints
- Code actions / quick fixes (insert `@SECURITY`, replace weak `xor`, etc.)
- Code lens for compile, convert to JSON/TOML, minify, and resolve
- Inline hex color picker (native VS Code color swatches + picker on every `#RRGGBB`/`#RGB` literal)
- 📅 Inline date/time picker — click the lens above any `Date`/`Timestamp` literal to edit it with a native date/datetime picker
- ▶ Blob preview — click the lens above any `b:(...)` literal to preview it as an image, audio, video, text, or hex dump, based on sniffed content
- Document formatting and folding

## Settings

| Setting | Description |
|---|---|
| `dixscript.server.path` | Absolute path to the `mdix-lsp` binary. Leave empty to use the bundled binary. |
| `dixscript.server.trace` | LSP protocol trace level (`off` / `messages` / `verbose`). |
| `dixscript.server.extraArgs` | Extra CLI arguments passed to `mdix-lsp`. |

## Links

- [DixScript-Rust on GitHub](https://github.com/Mid-D-Man/DixScript-Rust)
- [Original C# reference implementation](https://github.com/Mid-D-Man/DixScript)
