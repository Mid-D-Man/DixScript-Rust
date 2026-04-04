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
