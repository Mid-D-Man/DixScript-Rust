// mdix-lsp/src/features/commands.rs
//! Execute-command handler — runs when the user clicks a CodeLens or
//! invokes a command from the palette.
//!
//! All heavy work runs in `spawn_blocking` (called by server.rs) so
//! the async executor is never blocked.  Results are communicated back
//! via `window/showMessage`.
//!
//! ## Supported commands
//!
//! | Command               | Action                                          |
//! |-----------------------|-------------------------------------------------|
//! | `mdix.validate`       | Re-run pipeline, show error count               |
//! | `mdix.convertToJson`  | Write `<stem>.json` next to source file         |
//! | `mdix.convertToToml`  | Write `<stem>.toml` next to source file         |
//! | `mdix.minify`         | Write `<stem>.min.mdix` next to source file     |
//! | `mdix.compile`        | Shell out to `mdix compile <file>` (if on PATH) |
//! | `mdix.showAst`        | Show a short AST summary in a notification      |

use std::path::{Path, PathBuf};
use dixscript::Runtime::{DixConverter, DixFormatOptions};
use dixscript::Compiler::AST::DixScript;

use crate::features::formatting::format_source;

// ── Public result type ────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct CommandResult {
    /// Short headline shown in `window/showMessage`.
    pub message:  String,
    /// true = Info/success, false = Error
    pub success:  bool,
    /// Optional path of a file that was written.
    pub out_file: Option<PathBuf>,
}

impl CommandResult {
    fn ok(msg: impl Into<String>) -> Self {
        CommandResult { message: msg.into(), success: true, out_file: None }
    }
    fn ok_file(msg: impl Into<String>, path: PathBuf) -> Self {
        CommandResult { message: msg.into(), success: true, out_file: Some(path) }
    }
   pub fn err(msg: impl Into<String>) -> Self {
        CommandResult { message: msg.into(), success: false, out_file: None }
    }
}

// ── Validate ──────────────────────────────────────────────────────────────────

/// Returns a summary of the current diagnostic state for `source_path`.
/// The caller already has the error list; this formats a human message.
pub fn run_validate(error_count: usize, warning_count: usize) -> CommandResult {
    if error_count == 0 && warning_count == 0 {
        CommandResult::ok("✅ DixScript: file is valid — no errors or warnings.")
    } else if error_count == 0 {
        CommandResult::ok(format!(
            "⚠️ DixScript: {} warning(s), no errors.", warning_count
        ))
    } else {
        CommandResult::err(format!(
            "❌ DixScript: {} error(s), {} warning(s). Check the Problems panel.",
            error_count, warning_count
        ))
    }
}

// ── Convert → JSON ────────────────────────────────────────────────────────────

pub fn run_convert_to_json(
    ast:         &DixScript,
    source_path: Option<&Path>,
) -> CommandResult {
    let converter = DixConverter::new();

    match converter.to_json(ast, true) {
        Err(e) => CommandResult::err(format!("JSON conversion failed: {}", e)),
        Ok(json) => {
            if let Some(path) = source_path {
                let out = path.with_extension("json");
                match std::fs::write(&out, &json) {
                    Ok(()) => CommandResult::ok_file(
                        format!("→ JSON written to {}", out.display()),
                        out,
                    ),
                    Err(e) => CommandResult::err(format!("Could not write {}: {}", out.display(), e)),
                }
            } else {
                // No file path (e.g. untitled:) — just confirm conversion worked
                CommandResult::ok(format!(
                    "→ JSON ready ({} bytes). Save the file first to write output.",
                    json.len()
                ))
            }
        }
    }
}

// ── Convert → TOML ───────────────────────────────────────────────────────────

pub fn run_convert_to_toml(
    ast:         &DixScript,
    source_path: Option<&Path>,
) -> CommandResult {
    let converter = DixConverter::new();

    match converter.to_toml(ast) {
        Err(e) => CommandResult::err(format!("TOML conversion failed: {}", e)),
        Ok(toml) => {
            if let Some(path) = source_path {
                let out = path.with_extension("toml");
                match std::fs::write(&out, &toml) {
                    Ok(()) => CommandResult::ok_file(
                        format!("→ TOML written to {}", out.display()),
                        out,
                    ),
                    Err(e) => CommandResult::err(format!("Could not write {}: {}", out.display(), e)),
                }
            } else {
                CommandResult::ok(format!(
                    "→ TOML ready ({} bytes). Save the file first to write output.",
                    toml.len()
                ))
            }
        }
    }
}

// ── Minify ────────────────────────────────────────────────────────────────────

pub fn run_minify(
    ast:         &DixScript,
    source_path: Option<&Path>,
) -> CommandResult {
    let converter = DixConverter::new();
    let opts      = DixFormatOptions::minified();

    match converter.to_mdix(ast, Some(&opts)) {
        Err(e) => CommandResult::err(format!("Minify failed: {}", e)),
        Ok(minified) => {
            if let Some(path) = source_path {
                let stem    = path.file_stem().unwrap_or_default().to_string_lossy();
                let out     = path.with_file_name(format!("{}.min.mdix", stem));
                match std::fs::write(&out, &minified) {
                    Ok(()) => CommandResult::ok_file(
                        format!("⊡ Minified to {} ({} bytes)", out.display(), minified.len()),
                        out,
                    ),
                    Err(e) => CommandResult::err(format!("Could not write: {}", e)),
                }
            } else {
                CommandResult::ok(format!("⊡ Minified: {} bytes.", minified.len()))
            }
        }
    }
}

// ── Compile (shell out to mdix-cli) ──────────────────────────────────────────

pub fn run_compile(source_path: Option<&Path>) -> CommandResult {
    let path = match source_path {
        Some(p) => p,
        None => return CommandResult::err(
            "Save the file before compiling.".to_string()
        ),
    };

    // Try to find mdix on PATH
    let mdix_bin = which_mdix();

    match mdix_bin {
        None => CommandResult::err(concat!(
            "⚙ 'mdix' binary not found on PATH. ",
            "Build with `cargo build -p mdix-cli --release` and add to PATH."
        ).to_string()),
        Some(bin) => {
            let output = std::process::Command::new(&bin)
                .arg("compile")
                .arg(path)
                .output();

            match output {
                Err(e) => CommandResult::err(format!("Failed to run mdix: {}", e)),
                Ok(out) => {
                    if out.status.success() {
                        CommandResult::ok(format!(
                            "⚙ Compiled successfully: {}",
                            path.display()
                        ))
                    } else {
                        let stderr = String::from_utf8_lossy(&out.stderr);
                        CommandResult::err(format!(
                            "⚙ Compile failed:\n{}", stderr.trim()
                        ))
                    }
                }
            }
        }
    }
}

/// Try to locate the `mdix` binary on PATH or beside the LSP binary.
fn which_mdix() -> Option<PathBuf> {
    // 1. Check PATH
    if let Ok(path) = which::which("mdix") {
        return Some(path);
    }
    // 2. Check next to the running LSP binary
    if let Ok(exe) = std::env::current_exe() {
        let candidate = exe.with_file_name("mdix");
        if candidate.exists() {
            return Some(candidate);
        }
        // On Windows
        let candidate_exe = exe.with_file_name("mdix.exe");
        if candidate_exe.exists() {
            return Some(candidate_exe);
        }
    }
    None
}

// ── Show AST summary ──────────────────────────────────────────────────────────

pub fn run_show_ast(ast: &DixScript) -> CommandResult {
    let sections: Vec<String> = [
        ast.config.as_ref().map(|_| "@CONFIG"),
        ast.imports.as_ref().map(|_| "@IMPORTS"),
        ast.dlm.as_ref().map(|_| "@DLM"),
        ast.enums.as_ref().map(|_| "@ENUMS"),
        ast.quick_functions.as_ref().map(|_| "@QUICKFUNCS"),
        ast.data.as_ref().map(|_| "@DATA"),
        ast.security.as_ref().map(|_| "@SECURITY"),
    ]
    .iter()
    .filter_map(|o| o.as_ref().map(|s| s.to_string()))
    .collect();

    let func_count = ast.quick_functions.as_ref()
        .map(|qf| qf.functions.len())
        .unwrap_or(0);
    let enum_count = ast.enums.as_ref()
        .map(|e| e.enums.len())
        .unwrap_or(0);
    let data_entries = ast.data.as_ref()
        .map(|d| d.entries.len())
        .unwrap_or(0);

    CommandResult::ok(format!(
        "AST: sections=[{}]  QuickFuncs={}  Enums={}  DataEntries={}",
        sections.join(", "),
        func_count,
        enum_count,
        data_entries,
    ))
  }
