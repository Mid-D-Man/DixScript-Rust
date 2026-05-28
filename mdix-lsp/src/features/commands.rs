// mdix-lsp/src/features/commands.rs
//! Execute-command handler.

use std::path::{Path, PathBuf};
use dixscript::Runtime::{DixConverter, DixFormatOptions};
use dixscript::Compiler::AST::{DixScript, DLMModuleType, DLMModuleSubtype};

use crate::features::formatting::format_source;

#[derive(Debug)]
pub struct CommandResult {
    pub message:  String,
    pub success:  bool,
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

// ── Convert → JSON ────────────────────────────────────────────────────────────

pub fn run_convert_to_json(ast: &DixScript, source_path: Option<&Path>) -> CommandResult {
    let converter = DixConverter::new();
    match converter.to_json(ast, true) {
        Err(e) => CommandResult::err(format!("JSON conversion failed: {}", e)),
        Ok(json) => {
            if let Some(path) = source_path {
                let out = path.with_extension("json");
                match std::fs::write(&out, &json) {
                    Ok(()) => CommandResult::ok_file(
                        format!("→ JSON written to {}", out.display()), out,
                    ),
                    Err(e) => CommandResult::err(format!("Could not write {}: {}", out.display(), e)),
                }
            } else {
                CommandResult::ok(format!(
                    "→ JSON ready ({} bytes). Save the file first to write output.", json.len()
                ))
            }
        }
    }
}

// ── Convert → TOML ───────────────────────────────────────────────────────────

pub fn run_convert_to_toml(ast: &DixScript, source_path: Option<&Path>) -> CommandResult {
    let converter = DixConverter::new();
    match converter.to_toml(ast) {
        Err(e) => CommandResult::err(format!("TOML conversion failed: {}", e)),
        Ok(toml) => {
            if let Some(path) = source_path {
                let out = path.with_extension("toml");
                match std::fs::write(&out, &toml) {
                    Ok(()) => CommandResult::ok_file(
                        format!("→ TOML written to {}", out.display()), out,
                    ),
                    Err(e) => CommandResult::err(format!("Could not write {}: {}", out.display(), e)),
                }
            } else {
                CommandResult::ok(format!(
                    "→ TOML ready ({} bytes). Save the file first to write output.", toml.len()
                ))
            }
        }
    }
}

// ── Minify ────────────────────────────────────────────────────────────────────

pub fn run_minify(ast: &DixScript, source_path: Option<&Path>) -> CommandResult {
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
                        format!("⊡ Minified to {} ({} bytes)", out.display(), minified.len()), out,
                    ),
                    Err(e) => CommandResult::err(format!("Could not write: {}", e)),
                }
            } else {
                CommandResult::ok(format!("⊡ Minified: {} bytes.", minified.len()))
            }
        }
    }
}

// ── Create Resolved ───────────────────────────────────────────────────────────
//
// Outputs ONLY the @DATA section of the fully-resolved AST.
// The caller is responsible for passing a post-resolution AST (obtained via
// DixLoader::compile_to_resolved_ast so imports are properly loaded).

pub fn run_create_resolved(ast: &DixScript, source_path: Option<&Path>) -> CommandResult {
    // Only output the resolved @DATA section — QuickFuncs / imports are
    // compile-time artefacts and are not useful in the resolved output.
    let data_section = match &ast.data {
        Some(d) => d,
        None    => return CommandResult::err("⊞ No @DATA section found in the resolved AST."),
    };

    let output = format!("{}", data_section);

    if let Some(path) = source_path {
        let stem = path.file_stem().unwrap_or_default().to_string_lossy();
        let out  = path.with_file_name(format!("{}.resolved.mdix", stem));
        match std::fs::write(&out, &output) {
            Ok(()) => CommandResult::ok_file(
                format!(
                    "⊞ Resolved @DATA written to {} ({} bytes)",
                    out.display(),
                    output.len()
                ),
                out,
            ),
            Err(e) => CommandResult::err(format!("Could not write {}: {}", out.display(), e)),
        }
    } else {
        CommandResult::ok(format!(
            "⊞ Resolved: {} bytes. Save the file first to write output.",
            output.len()
        ))
    }
}

// ── Compile ───────────────────────────────────────────────────────────────────

fn describe_dlm_outputs(ast: &DixScript, source_path: &Path) -> Option<String> {
    let dlm = ast.dlm.as_ref()?;
    if dlm.modules.is_empty() { return None; }

    let dir  = source_path.parent().map(|p| p.display().to_string()).unwrap_or_else(|| ".".to_string());
    let stem = source_path.file_stem().unwrap_or_default().to_string_lossy();

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
        parts.push(format!("📦 `{}/{}.mdix.enc` — encrypted/compressed binary", dir, stem));
        parts.push(format!("🔑 `{}/{}.mdix.key` — decryption key file (keep this safe!)", dir, stem));
    }
    if has_auditor {
        parts.push(format!("📋 `{}/{}.mdix.au` — audit/checksum file", dir, stem));
    }
    if parts.is_empty() { return None; }

    Some(format!(
        "\n\n⚙️ **@DLM detected** — the following files will be written to `{}`:\n{}",
        dir, parts.join("\n")
    ))
}

pub fn run_compile(source_path: Option<&Path>, ast: Option<&DixScript>) -> CommandResult {
    let path = match source_path {
        Some(p) => p,
        None    => return CommandResult::err("Save the file before compiling."),
    };

    let dlm_notice = ast.and_then(|a| describe_dlm_outputs(a, path)).unwrap_or_default();
    let mdix_bin   = which_mdix();

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
            let output = std::process::Command::new(&bin).arg("compile").arg(path).output();
            match output {
                Err(e) => CommandResult::err(format!("Failed to run mdix: {}", e)),
                Ok(out) => {
                    if out.status.success() {
                        CommandResult::ok(format!("⚙ Compiled successfully: {}{}", path.display(), dlm_notice))
                    } else {
                        let stderr = String::from_utf8_lossy(&out.stderr);
                        CommandResult::err(format!("⚙ Compile failed:\n{}{}", stderr.trim(), dlm_notice))
                    }
                }
            }
        }
    }
}

fn which_mdix() -> Option<PathBuf> {
    if let Ok(path) = which::which("mdix") { return Some(path); }
    if let Ok(exe) = std::env::current_exe() {
        let c = exe.with_file_name("mdix");
        if c.exists() { return Some(c); }
        let c = exe.with_file_name("mdix.exe");
        if c.exists() { return Some(c); }
    }
    None
}

// ── Show AST ──────────────────────────────────────────────────────────────────

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

    let func_count   = ast.quick_functions.as_ref().map(|qf| qf.functions.len()).unwrap_or(0);
    let enum_count   = ast.enums.as_ref().map(|e| e.enums.len()).unwrap_or(0);
    let data_entries = ast.data.as_ref().map(|d| d.entries.len()).unwrap_or(0);

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
        sections.join(", "), func_count, enum_count, data_entries, dlm_summary,
    ))
                }
