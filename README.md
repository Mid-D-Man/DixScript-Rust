# DixScript - MidManStudio

**Secure Data Interchange Format with Built-in Encryption**

## Namespace
`MidManStudio::DixScript`

## Naming Convention
This project uses **C# naming conventions** for maximum compatibility:
- Types: `PascalCase` (e.g., `DixData`, `TokenType`)
- Methods: `PascalCase` (e.g., `ParseFile`, `GetValue`)
- Fields: `PascalCase` (e.g., `Line`, `Column`)

## Project Structure
```
src/
├── DixCore/          # C#-like collection wrappers
│   ├── ImmutableArray.rs
│   ├── List.rs
│   ├── Dictionary.rs
│   └── HashSet.rs
├── Utilities/        # Core utilities (Result, Token, Logger, etc.)
├── Builtins/         # Built-in types and methods
├── Compiler/         # Compilation pipeline
│   ├── AST/
│   ├── Core/
│   ├── DLM/
│   └── ...
└── Runtime/          # Runtime API
```

## Building
```bash
# Check for errors
cargo check

# Build debug
cargo build

# Build release
cargo build --release

# Run tests
cargo test
```

## Development

Open in RustRover:
1. File → Open → Select `DixScript-Rust` folder
2. RustRover auto-detects Cargo.toml
3. Build: Ctrl+F9

## License
MIT
