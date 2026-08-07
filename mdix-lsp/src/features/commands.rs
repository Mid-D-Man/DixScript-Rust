//! Execute-command handler.

use std::path::{Path, PathBuf};
use dixscript::Runtime::{DixConverter, DixFormatOptions};
use dixscript::Compiler::AST::{DixScript, DLMModuleType, DataEntry, Value};

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
// with no @ENUMS and no @IMPORTS to back a symbolic reference. There's no
// declaration to point it at here, so the only output that means anything on
// its own is the literal resolved value -- the same call
// `DixFormatOptions::inline_enum_values` makes `to_mdix` do. Both go through
// `DixConverter::inline_enum_values` now, so there's exactly one
// implementation of "resolve every Value::EnumValue to the literal it stands
// for" in the whole codebase, not a copy per call site.

pub fn run_create_resolved(ast: &DixScript, source_path: Option<&Path>) -> CommandResult {
    // Only output the resolved @DATA section — QuickFuncs / imports are
    // compile-time artefacts and are not useful in the resolved output.
    let data_section = match &ast.data {
        Some(d) => d,
        None    => return CommandResult::err("⊞ No @DATA section found in the resolved AST."),
    };
    let converter = DixConverter::new();
    let flattened = converter.inline_enum_values(data_section, ast.enums.as_ref());

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

// ── Theme colors (semantic token color customization) ─────────────────────────
//
// Reads `@DATA` tables named `dark:` / `light:` and maps their properties to
// VS Code's `editor.semanticTokenColorCustomizations` "rules" shape. Property
// names use an `m_` prefix (`m_keyword`, `m_string`, ...): `parse_property_name`
// (data_section_parser.rs) does accept a bare reserved word like `string` as a
// table-property key today, so the prefix isn't rescuing a parse failure — it's
// what this file's `map_theme_key` table below actually keys off of, and it
// keeps working even if that keyword-tolerance is ever tightened later.
//
// Mapping mirrors capabilities.rs's TOKEN_TYPES legend exactly (the 14 in
// active use, plus the STRUCT/REGEXP/EVENT/METHOD slots that are registered
// but not yet emitted by any highlighter). A new legend entry there should
// come with a new arm here.
//
// This command deliberately does NOT return CommandResult / go through
// self.show_message() like everything else in this file. Every other command
// here is a one-shot action reported via a toast; this one hands structured
// data back to a client-side command (mdix.applyThemeColors, registered in
// the VS Code extension, deliberately NOT in ALL_COMMANDS) which does the
// actual workspace.getConfiguration().update(...) and reports success or
// failure itself. A toast here too would just double up on the same action.

const MDIX_KEY_PREFIX: &str = "m_";

fn map_theme_key(name: &str) -> Option<&'static str> {
    let bare = name.strip_prefix(MDIX_KEY_PREFIX)?;
    Some(match bare {
        "keyword"    => "keyword",
        "string"     => "string",
        "number"     => "number",
        "operator"   => "operator",
        "variable"   => "variable",
        "function"   => "function",
        "type"       => "type",
        "enummember" => "enumMember",
        "comment"    => "comment",
        "namespace"  => "namespace",
        "property"   => "property",
        "parameter"  => "parameter",
        "macro"      => "macro",
        "decorator"  => "decorator",
        "struct"     => "struct",
        "regexp"     => "regexp",
        "event"      => "event",
        "method"     => "method",
        _ => return None,
    })
}

/// Cheap presence check used to gate the "🎨 Apply Theme" CodeLens — true if
/// `@DATA` has a top-level `dark:` and/or `light:` table. Doesn't validate
/// keys or values; `run_get_theme_colors` is the source of truth for that.
pub fn has_theme_tables(ast: &DixScript) -> bool {
    ast.data.as_ref().is_some_and(|d| {
        d.entries.iter().any(|e| matches!(
            e,
            DataEntry::TableProperty { path, .. }
                if path.segments.len() == 1
                    && matches!(path.segments[0].as_str(), "dark" | "light")
        ))
    })
}

fn hex_from_value(value: &Value) -> Option<String> {
    match value {
        Value::HexColor { value, .. } => Some(value.clone()),
        // Tolerate a quoted "#RRGGBB" string too — cheap to accept, no reason
        // to make someone re-type a color just because they quoted it.
        Value::String { value, .. } if value.starts_with('#') => Some(value.clone()),
        _ => None,
    }
}

pub fn run_get_theme_colors(ast: &DixScript) -> serde_json::Value {
    use serde_json::{Map, Value as JsonValue};

    let mut dark:  Map<String, JsonValue> = Map::new();
    let mut light: Map<String, JsonValue> = Map::new();
    let mut warnings: Vec<String> = Vec::new();

    if let Some(data) = ast.data.as_ref() {
        for entry in &data.entries {
            let DataEntry::TableProperty { path, properties, .. } = entry else { continue };
            if path.segments.len() != 1 {
                continue;
            }

            let bucket = match path.segments[0].as_str() {
                "dark"  => &mut dark,
                "light" => &mut light,
                _       => continue,
            };

            for prop in properties {
                let Some(token_type) = map_theme_key(&prop.name) else {
                    warnings.push(format!(
                        "unrecognized key '{}' (expected an m_-prefixed token name, e.g. m_keyword)",
                        prop.name
                    ));
                    continue;
                };
                match hex_from_value(&prop.value) {
                    Some(hex) => { bucket.insert(token_type.to_string(), JsonValue::String(hex)); }
                    None => warnings.push(format!(
                        "'{}' is not a hex color (got '{}') — use e.g. `{} = #569CD6`",
                        prop.name, prop.value, prop.name
                    )),
                }
            }
        }
    }

    let success = !dark.is_empty() || !light.is_empty();
    let message = if success {
        format!("🎨 dark: {} color(s), light: {} color(s)", dark.len(), light.len())
    } else {
        "🎨 No `dark:` or `light:` table found in @DATA — expected e.g. \
         `dark: m_keyword = #569CD6, m_string = #CE9178`.".to_string()
    };

    serde_json::json!({
        "success":  success,
        "message":  message,
        "dark":     if dark.is_empty()  { JsonValue::Null } else { JsonValue::Object(dark) },
        "light":    if light.is_empty() { JsonValue::Null } else { JsonValue::Object(light) },
        "warnings": warnings,
    })
}

// ── Settings (bulk-apply a curated set of VS Code / DixScript settings) ────────
//
// Same shape as the theme-colors command above: reads @DATA's `settings:`
// table, maps `m_`-prefixed keys through a curated table (`map_setting_key`),
// returns JSON for `mdix.applySettings` (client-only, deliberately not in
// ALL_COMMANDS) to apply via the VS Code configuration API.
//
// Curated allowlist, not arbitrary keys: a `.mdix` key here can only ever
// reach one of the specific VS Code settings named below, decided in Rust,
// not whatever dotted string someone puts in the file. A new setting needs a
// new match arm — deliberately more friction than a shared/committed
// settings file being able to silently rewrite any global VS Code setting.
//
// `dixscript.server.path` is deliberately NOT in this table — it's an
// absolute filesystem path to the mdix-lsp binary, specific to one machine.
// Applying it from a shared file would just break the extension on anyone
// else's machine (or your own, on a different device).

#[derive(Clone, Copy)]
enum SettingScope {
    /// Plain global (User) setting — dixscript.server.*.
    Global,
    /// A generic `editor.*` setting, written scoped to the `mdix` language
    /// via VS Code's `overrideInLanguage` update flag, not globally — so it
    /// only affects .mdix files, not every language you edit.
    LanguageMdix,
}

#[derive(Clone, Copy)]
enum SettingKind {
    Str,
    Bool,
    StrArray,
}

struct SettingSpec {
    vscode_key: &'static str,
    scope:      SettingScope,
    kind:       SettingKind,
}

fn map_setting_key(name: &str) -> Option<SettingSpec> {
    let bare = name.strip_prefix(MDIX_KEY_PREFIX)?;
    Some(match bare {
        "trace" => SettingSpec {
            vscode_key: "dixscript.server.trace", scope: SettingScope::Global, kind: SettingKind::Str,
        },
        "extra_args" => SettingSpec {
            vscode_key: "dixscript.server.extraArgs", scope: SettingScope::Global, kind: SettingKind::StrArray,
        },
        "inlay_hints" => SettingSpec {
            vscode_key: "editor.inlayHints.enabled", scope: SettingScope::LanguageMdix, kind: SettingKind::Str,
        },
        "semantic_highlighting" => SettingSpec {
            vscode_key: "editor.semanticHighlighting.enabled", scope: SettingScope::LanguageMdix, kind: SettingKind::Str,
        },
        "format_on_save" => SettingSpec {
            vscode_key: "editor.formatOnSave", scope: SettingScope::LanguageMdix, kind: SettingKind::Bool,
        },
        "word_wrap" => SettingSpec {
            vscode_key: "editor.wordWrap", scope: SettingScope::LanguageMdix, kind: SettingKind::Str,
        },
        _ => return None,
    })
}

/// Cheap presence check used to gate the "⚙ Apply Settings" CodeLens — true
/// if @DATA has a top-level `settings:` table. Doesn't validate keys/values;
/// `run_get_settings_values` is the source of truth for that.
pub fn has_settings_table(ast: &DixScript) -> bool {
    ast.data.as_ref().is_some_and(|d| {
        d.entries.iter().any(|e| matches!(
            e,
            DataEntry::TableProperty { path, .. }
                if path.segments.len() == 1 && path.segments[0] == "settings"
        ))
    })
}

fn json_for_setting(kind: SettingKind, value: &Value) -> Result<serde_json::Value, String> {
    use serde_json::Value as JsonValue;
    match (kind, value) {
        (SettingKind::Str, Value::String { value, .. }) => Ok(JsonValue::String(value.clone())),
        (SettingKind::Bool, Value::Boolean { value, .. }) => Ok(JsonValue::Bool(*value)),
        (SettingKind::StrArray, Value::Array { values, .. }) => {
            let mut out = Vec::with_capacity(values.len());
            for v in values {
                match v {
                    Value::String { value, .. } => out.push(JsonValue::String(value.clone())),
                    other => return Err(format!("array element '{}' is not a string", other)),
                }
            }
            Ok(JsonValue::Array(out))
        }
        (SettingKind::Str, other) => Err(format!("expected a string, got '{}'", other)),
        (SettingKind::Bool, other) => Err(format!("expected true/false, got '{}'", other)),
        (SettingKind::StrArray, other) => Err(format!("expected an array of strings, got '{}'", other)),
    }
}

pub fn run_get_settings_values(ast: &DixScript) -> serde_json::Value {
    use serde_json::Value as JsonValue;

    let mut settings: Vec<JsonValue> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    if let Some(data) = ast.data.as_ref() {
        for entry in &data.entries {
            let DataEntry::TableProperty { path, properties, .. } = entry else { continue };
            if path.segments.len() != 1 || path.segments[0] != "settings" {
                continue;
            }

            for prop in properties {
                let Some(spec) = map_setting_key(&prop.name) else {
                    warnings.push(format!(
                        "unrecognized key '{}' (expected an m_-prefixed setting name, e.g. m_inlay_hints)",
                        prop.name
                    ));
                    continue;
                };
                match json_for_setting(spec.kind, &prop.value) {
                    Ok(json_value) => {
                        let scope = match spec.scope {
                            SettingScope::Global       => "global",
                            SettingScope::LanguageMdix => "mdix",
                        };
                        settings.push(serde_json::json!({
                            "key":   spec.vscode_key,
                            "scope": scope,
                            "value": json_value,
                        }));
                    }
                    Err(reason) => warnings.push(format!("'{}' — {}", prop.name, reason)),
                }
            }
        }
    }

    let success = !settings.is_empty();
    let message = if success {
        format!("⚙ {} setting(s) ready to apply", settings.len())
    } else {
        "⚙ No `settings:` table found in @DATA — expected e.g. \
         `settings: m_inlay_hints = \"off\", m_format_on_save = true`.".to_string()
    };

    serde_json::json!({
        "success":  success,
        "message":  message,
        "settings": settings,
        "warnings": warnings,
    })
}
