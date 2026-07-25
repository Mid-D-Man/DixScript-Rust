//! Execute-command handler.

use std::path::{Path, PathBuf};
use dixscript::Runtime::{DixConverter, DixFormatOptions};
use dixscript::Compiler::AST::{
    DixScript, DLMModuleType, DataSection, DataEntry, PropertyAssignment, Value, EnumsSection,
    ObjectProperty,
};

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

// ── Sanitise resolved .mdix output ───────────────────────────────────────────
//
// The `DataSection` Display implementation unconditionally appends a `,` after
// every entry — simple properties, table properties, and group-array items —
// including the very last one in each block.  When the resolved file is
// re-compiled or analysed by the LSP the strict parser rejects these trailing
// commas ("found comma before next data entry").
//
// This function strips the trailing entry-separator comma from every data-entry
// line.  It is safe because, in the one-entry-per-line resolved format, all
// commas that are *internal* to a value (inside `{…}`, `t:(…)`, `[…]`) always
// appear before the last character on the line — the line-final `,` is always
// the inter-entry separator added by the Display impl.
//
// Lines that are section delimiters (`@DATA(`, `)`) or comment lines are left
// untouched.

fn sanitize_resolved_mdix(raw: &str) -> String {
    let mut result = String::with_capacity(raw.len());

    for line in raw.lines() {
        let trimmed = line.trim_end();

        // Structural lines: section header, closing paren, blank, comment.
        let is_structural = trimmed.is_empty()
            || trimmed.trim_start().starts_with("//")
            || trimmed.trim_start().starts_with('@')
            || trimmed.trim_start() == ")";

        if !is_structural && trimmed.ends_with(',') {
            // Drop the trailing entry-separator comma.
            result.push_str(&trimmed[..trimmed.len() - 1]);
        } else {
            result.push_str(trimmed);
        }
        result.push('\n');
    }

    result
}

// ── Create Resolved ───────────────────────────────────────────────────────────
//
// Outputs ONLY the @DATA section of the fully-resolved AST.
// The caller is responsible for passing a post-resolution AST (obtained via
// DixLoader::compile_to_resolved_ast so imports are properly loaded).
//
// `Value::EnumValue` deliberately stays symbolic ("EnumMan.Suka.Crack")
// everywhere else in the compiler -- that's what lets `DixConverter` and
// `DixData` still report the enum's name/field alongside its value. But this
// command only ever emits @DATA, by design ("QuickFuncs / imports are
// compile-time artefacts and are not useful in the resolved output" below),
// with no @ENUMS and no @IMPORTS to back a symbolic reference — the same
// class of problem `to_mdix` had before it started flattening imported enum
// *declarations* to valid local ones. There's no declaration to flatten TO
// here at all, so the only output that means anything on its own is the
// literal resolved value. `flatten_enum_values_for_resolved_output` swaps
// every `Value::EnumValue` for the `Value::Integer` it already resolved to,
// via the same `ast.enums` table (including the merged-in qualified imports)
// that `DixConverter` reads from — so a plain literal like `9` comes out
// instead of the unusable qualified identifier the AST's generic `Display`
// impl would otherwise print (`Value::Display` is intentionally left
// printing the symbolic form globally -- that's correct for debug/hover
// output elsewhere in the LSP; this is a local, purpose-specific transform,
// not a change to that shared impl).

fn resolve_enum_int(enums: Option<&EnumsSection>, enum_name: &str, field_name: &str) -> Option<i32> {
    enums?
        .enums
        .iter()
        .find(|decl| decl.name == enum_name)?
        .fields
        .iter()
        .find(|f| f.name == field_name)?
        .value
}

fn flatten_value(value: &Value, enums: Option<&EnumsSection>) -> Value {
    match value {
        Value::EnumValue { enum_name, value: field_name, position } => {
            match resolve_enum_int(enums, enum_name, field_name) {
                Some(int_value) => Value::Integer { value: int_value, position: *position },
                // Shouldn't happen -- compilation already validated every
                // enum reference -- but leave it symbolic rather than
                // silently emitting a wrong number if it ever does.
                None => value.clone(),
            }
        }
        Value::Array { values, position } => Value::Array {
            values: values.iter().map(|v| flatten_value(v, enums)).collect(),
            position: *position,
        },
        Value::NestedArray { values, level, position } => Value::NestedArray {
            values: values.iter().map(|v| flatten_value(v, enums)).collect(),
            level: *level,
            position: *position,
        },
        Value::Object { properties, position } => Value::Object {
            properties: properties
                .iter()
                .map(|p| ObjectProperty {
                    key: p.key.clone(),
                    value: flatten_value(&p.value, enums),
                    position: p.position,
                })
                .collect(),
            position: *position,
        },
        Value::PrefixedConstructor { prefix, arguments, position } => Value::PrefixedConstructor {
            prefix: prefix.clone(),
            arguments: arguments.iter().map(|v| flatten_value(v, enums)).collect(),
            position: *position,
        },
        other => other.clone(),
    }
}

fn flatten_enum_values_for_resolved_output(data: &DataSection, enums: Option<&EnumsSection>) -> DataSection {
    let entries = data
        .entries
        .iter()
        .map(|entry| match entry {
            DataEntry::SimpleProperty { name, data_type, value, position } => DataEntry::SimpleProperty {
                name: name.clone(),
                data_type: data_type.clone(),
                value: flatten_value(value, enums),
                position: *position,
            },
            DataEntry::TableProperty { path, properties, position } => DataEntry::TableProperty {
                path: path.clone(),
                properties: properties
                    .iter()
                    .map(|p| PropertyAssignment {
                        name: p.name.clone(),
                        data_type: p.data_type.clone(),
                        value: flatten_value(&p.value, enums),
                        position: p.position,
                    })
                    .collect(),
                position: *position,
            },
            DataEntry::GroupArray { path, items, position } => DataEntry::GroupArray {
                path: path.clone(),
                items: items.iter().map(|v| flatten_value(v, enums)).collect(),
                position: *position,
            },
            DataEntry::ObjectProperty { name, data_type, object, position } => DataEntry::ObjectProperty {
                name: name.clone(),
                data_type: data_type.clone(),
                object: Box::new(flatten_value(object, enums)),
                position: *position,
            },
        })
        .collect();

    DataSection { entries, position: data.position }
}

pub fn run_create_resolved(ast: &DixScript, source_path: Option<&Path>) -> CommandResult {
    // Only output the resolved @DATA section — QuickFuncs / imports are
    // compile-time artefacts and are not useful in the resolved output.
    let data_section = match &ast.data {
        Some(d) => d,
        None    => return CommandResult::err("⊞ No @DATA section found in the resolved AST."),
    };
    let flattened = flatten_enum_values_for_resolved_output(data_section, ast.enums.as_ref());

    // Generate the raw Display output then strip the trailing commas that the
    // DataSection Display implementation appends after every entry (including
    // the last).  Without this step the resolved file fails to re-compile with
    // "TRAILING COMMA: Found comma before next data entry."
    let raw    = format!("{}", flattened);
    let output = sanitize_resolved_mdix(&raw);

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
