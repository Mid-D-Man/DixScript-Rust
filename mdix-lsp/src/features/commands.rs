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
use dixscript::Compiler::AST::{DixScript, DLMModuleType, DLMModuleSubtype};

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

/// Returns a summary of the current diagnostic state.
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
                    Err(e) => CommandResult::err(
                        format!("Could not write {}: {}", out.display(), e)
                    ),
                }
            } else {
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
                    Err(e) => CommandResult::err(
                        format!("Could not write {}: {}", out.display(), e)
                    ),
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
                let stem = path.file_stem().unwrap_or_default().to_string_lossy();
                let out  = path.with_file_name(format!("{}.min.mdix", stem));
                match std::fs::write(&out, &minified) {
                    Ok(()) => CommandResult::ok_file(
                        format!(
                            "⊡ Minified to {} ({} bytes)",
                            out.display(), minified.len()
                        ),
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

/// Inspect the AST's @DLM section and return a description of what output
/// files the compiler will produce alongside the source file.
///
/// Returns `None` when there are no DLM modules (plain compile, no side files).
fn describe_dlm_outputs(ast: &DixScript, source_path: &Path) -> Option<String> {
    let dlm = ast.dlm.as_ref()?;
    if dlm.modules.is_empty() {
        return None;
    }

    let dir = source_path
        .parent()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| ".".to_string());

    let stem = source_path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy();

    let mut parts: Vec<String> = Vec::new();
    let mut has_encryptor   = false;
    let mut has_compressor  = false;
    let mut has_auditor     = false;

    for module in &dlm.modules {
        match module.module_type {
            DLMModuleType::DEncryptor  => has_encryptor  = true,
            DLMModuleType::DCompressor => has_compressor = true,
            DLMModuleType::DAuditor    => has_auditor    = true,
            _                          => {}
        }
    }

    if has_encryptor || has_compressor {
        // Primary encrypted/compressed output
        parts.push(format!(
            "📦 `{}/{}.mdix.enc` — encrypted/compressed binary",
            dir, stem
        ));
        // Key file is always produced alongside .enc
        parts.push(format!(
            "🔑 `{}/{}.mdix.key` — decryption key file (keep this safe!)",
            dir, stem
        ));
    }

    if has_auditor {
        parts.push(format!(
            "📋 `{}/{}.mdix.au` — audit/checksum file",
            dir, stem
        ));
    }

    if parts.is_empty() {
        return None;
    }

    Some(format!(
        "\n\n⚙️ **@DLM detected** — the following files will be written to `{}`:\n{}",
        dir,
        parts.join("\n")
    ))
}

pub fn run_compile(source_path: Option<&Path>, ast: Option<&DixScript>) -> CommandResult {
    let path = match source_path {
        Some(p) => p,
        None => return CommandResult::err("Save the file before compiling."),
    };

    // Build the DLM output-files notice before we shell out, so the user
    // knows what to expect even if the binary is not on PATH yet.
    let dlm_notice = ast
        .and_then(|a| describe_dlm_outputs(a, path))
        .unwrap_or_default();

    let mdix_bin = which_mdix();

    match mdix_bin {
        None => CommandResult::err(format!(
            concat!(
                "⚙ 'mdix' binary not found on PATH. ",
                "Build with `cargo build -p mdix-cli --release` and add to PATH.",
                "{}",
            ),
            dlm_notice
        )),

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
                            "⚙ Compiled successfully: {}{}",
                            path.display(),
                            dlm_notice
                        ))
                    } else {
                        let stderr = String::from_utf8_lossy(&out.stderr);
                        CommandResult::err(format!(
                            "⚙ Compile failed:\n{}{}",
                            stderr.trim(),
                            dlm_notice
                        ))
                    }
                }
            }
        }
    }
}

/// Try to locate the `mdix` binary on PATH or beside the running LSP binary.
fn which_mdix() -> Option<PathBuf> {
    // 1. Check PATH
    if let Ok(path) = which::which("mdix") {
        return Some(path);
    }
    // 2. Check next to the running LSP binary
    if let Ok(exe) = std::env::current_exe() {
        let candidate = exe.with_file_name("mdix");
        if candidate.exists() { return Some(candidate); }
        let candidate_exe = exe.with_file_name("mdix.exe");
        if candidate_exe.exists() { return Some(candidate_exe); }
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

    // Summarise DLM modules when present
    let dlm_summary = ast.dlm.as_ref().map(|dlm| {
        let mods: Vec<String> = dlm.modules.iter().map(|m| {
            match m.subtype {
                Some(st) => format!("{}.{}", m.module_type, st),
                None     => format!("{}", m.module_type),
            }
        }).collect();
        format!("  DLM=[{}]", mods.join(", "))
    }).unwrap_or_default();

    CommandResult::ok(format!(
        "AST: sections=[{}]  QuickFuncs={}  Enums={}  DataEntries={}{}",
        sections.join(", "),
        func_count,
        enum_count,
        data_entries,
        dlm_summary,
    ))
}
