// mdix-cli/src/commands/debug_symbols.rs
//! `mdix debug-symbols <file>` — dump the symbol table produced by semantic analysis.
//!
//! Uses Approach B (tokenizer-first): tokenize full source → split @CONFIG
//! → process config tokens → parse rest tokens → semantic analysis.
//!
//! Use this to verify:
//!   - Enum definitions are correctly registered with field values.
//!   - QuickFunc signatures, return types, and scope bindings are correct.
//!   - DATA variables are indexed with correct paths and types.
//!   - Imported namespaces are resolved.
//!   - Builtin statics are registered.

use std::path::PathBuf;
use clap::Args;
use dixscript::Compiler::Core::Tokenizer::{Tokenizer, split_config_tokens};
use dixscript::Compiler::Core::Config::{ConfigSectionHandler, OperationalSettings};
use dixscript::Compiler::Core::{GeneralParser, GeneralSemanticAnalyzer};
use dixscript::Compiler::Utilities::SymbolTable;
use crate::commands::GlobalOpts;
use crate::services::file_io;

#[derive(Args)]
pub struct DebugSymbolsArgs {
    /// Path to the .mdix file
    pub file: PathBuf,

    /// Write output to this file instead of stdout
    #[arg(short, long)]
    pub output: Option<String>,

    /// Section filter: ENUMS | FUNCTIONS | DATA | NAMESPACES | BUILTINS | CONFIG | ALL
    #[arg(long, default_value = "ALL")]
    pub section: String,

    /// Show positions and extra detail
    #[arg(long)]
    pub verbose: bool,
}

pub fn run(args: DebugSymbolsArgs, _global: &GlobalOpts) -> i32 {
    let source = match file_io::read_file(&args.file) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error: {}", e);
            return 2;
        }
    };

    // ── Approach B pipeline ───────────────────────────────────────────────

    // Stage 1: tokenize full source with default settings.
    let initial_settings = OperationalSettings {
        source_file_path: Some(args.file.to_string_lossy().to_string()),
        ..OperationalSettings::default()
    };
    let tokenizer = Tokenizer::new(&source, &initial_settings);
    let tok_result = tokenizer.tokenize();

    // Stage 2: split @CONFIG and process it.
    let split = split_config_tokens(tok_result.tokens);
    let mut config_handler = ConfigSectionHandler::new(None);
    let config_result = config_handler.process_config_tokens(&split.config_tokens);

    let mut settings = config_result.operational_settings.clone();
    settings.source_file_path = Some(args.file.to_string_lossy().to_string());

    // Stage 3: parse the rest of the token stream.
    let parser = match GeneralParser::new(
        split.rest_tokens,
        &config_result.config_section,
        &settings,
    ) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Parser error: {}", e);
            return 1;
        }
    };

    let ast = match parser.parse() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("Parse error: {}", e);
            return 1;
        }
    };

    // Stage 4: semantic analysis.
    let analyzer = GeneralSemanticAnalyzer::new(&ast, &settings);
    let sem = analyzer.analyze();

    if !sem.is_success {
        eprintln!(
            "⚠  Semantic errors ({}) — symbol table may be incomplete:",
            sem.errors.len()
        );
        for e in sem.errors.iter().take(8) {
            eprintln!("   [{}] {}", e.error_id, e.message);
        }
        if sem.errors.len() > 8 {
            eprintln!("   … and {} more", sem.errors.len() - 8);
        }
        eprintln!();
    }

    let st = match &sem.symbol_table {
        None => {
            eprintln!("No symbol table produced (semantic analysis failed entirely).");
            return 1;
        }
        Some(st) => st,
    };

    let out = format_symbol_table(st, &args);

    match &args.output {
        Some(path) => {
            if let Err(e) = std::fs::write(path, &out) {
                eprintln!("Failed to write output: {}", e);
                return 1;
            }
            println!("Symbol table written to: {}", path);
        }
        None => print!("{}", out),
    }

    0
}

// ── Formatter ─────────────────────────────────────────────────────────────────

fn format_symbol_table(st: &SymbolTable, args: &DebugSymbolsArgs) -> String {
    let mut out = String::new();
    let section = args.section.to_uppercase();
    let show_all = section == "ALL";
    let verbose = args.verbose;

    out.push_str(&format!(
        "=== DixScript Symbol Table ===\nTotal symbols: {}\n\n",
        st.get_total_symbols()
    ));

    // ── ENUMS ─────────────────────────────────────────────────────────────────
    if show_all || section == "ENUMS" {
        out.push_str(&format!("── ENUMS ({}) {}\n", st.enums.len(), "─".repeat(52)));
        if st.enums.is_empty() {
            out.push_str("  (none)\n");
        } else {
            let mut names: Vec<&String> = st.enums.keys().collect();
            names.sort();
            for name in names {
                let fields = &st.enums[name];
                out.push_str(&format!("  {}\n", name));
                let mut pairs: Vec<(&String, &i32)> = fields.iter().collect();
                pairs.sort_by_key(|(_, v)| *v);
                for (field, value) in &pairs {
                    out.push_str(&format!("    {:<28} = {}\n", field, value));
                }
            }
        }
        out.push('\n');
    }

    // ── QUICKFUNCS / FUNCTIONS ────────────────────────────────────────────────
    if show_all || section == "FUNCTIONS" {
        out.push_str(&format!(
            "── QUICKFUNCS ({}) {}\n",
            st.functions.len(),
            "─".repeat(45)
        ));
        if st.functions.is_empty() {
            out.push_str("  (none)\n");
        } else {
            let mut names: Vec<&String> = st.functions.keys().collect();
            names.sort();
            for name in names {
                let sig = &st.functions[name];
                let ret = sig.return_type
                    .map(|t| format!("{}", t))
                    .unwrap_or_else(|| "?".to_string());
                let params: Vec<String> = sig.parameters.iter().map(|p| {
                    let t = p.param_type.map(|dt| format!("<{}>", dt)).unwrap_or_default();
                    let d = if p.has_default_value && verbose { " = …" } else { "" };
                    format!("{}{}{}", p.name, t, d)
                }).collect();
                let scope = if sig.scopes.is_empty() {
                    "global".to_string()
                } else {
                    sig.scopes.join(", ")
                };
                out.push_str(&format!(
                    "  ~{}<{}> => {}({})\n",
                    name, ret, scope,
                    params.join(", ")
                ));
                if verbose && sig.line > 0 {
                    out.push_str(&format!("    @L{}:C{}\n", sig.line, sig.column));
                }
            }
        }
        out.push('\n');

        if !st.dix_functions.is_empty() {
            out.push_str(&format!(
                "  ── DIX FUNCTIONS ({}) ──\n",
                st.dix_functions.len()
            ));
            let mut dix_names: Vec<&String> = st.dix_functions.keys().collect();
            dix_names.sort();
            for name in dix_names {
                let sig = &st.dix_functions[name];
                out.push_str(&format!(
                    "  Dix.{}({}) → {}\n",
                    sig.name,
                    sig.parameter_types.join(", "),
                    sig.return_type
                ));
            }
            out.push('\n');
        }
    }

    // ── DATA VARIABLES ────────────────────────────────────────────────────────
    if show_all || section == "DATA" {
        out.push_str(&format!(
            "── DATA VARIABLES ({}) {}\n",
            st.data_variables.len(),
            "─".repeat(40)
        ));
        if st.data_variables.is_empty() {
            out.push_str("  (none)\n");
        } else {
            out.push_str(&format!(
                "  {:<46} {:<14} {:<14} {}\n",
                "Path", "Declared", "Inferred", "Source"
            ));
            out.push_str(&format!("  {}\n", "─".repeat(82)));

            let mut paths: Vec<&String> = st.data_variables.keys().collect();
            paths.sort();
            for path in paths {
                let var = &st.data_variables[path];
                let decl = var.declared_type
                    .map(|t| format!("{}", t))
                    .unwrap_or_else(|| "─".to_string());
                let inf = var.inferred_type
                    .map(|t| format!("{}", t))
                    .unwrap_or_else(|| "─".to_string());
                let src = if var.is_inferred { "inferred" } else { "explicit" };
                out.push_str(&format!(
                    "  {:<46} {:<14} {:<14} {}\n",
                    path, decl, inf, src
                ));
                if verbose && var.line > 0 {
                    out.push_str(&format!(
                        "  {:<46} @L{}:C{}  scope: {}\n",
                        "", var.line, var.column, var.scope
                    ));
                }
            }
        }
        out.push('\n');
    }

    // ── NAMESPACES ────────────────────────────────────────────────────────────
    if show_all || section == "NAMESPACES" {
        out.push_str(&format!(
            "── NAMESPACES ({}) {}\n",
            st.namespaces.len(),
            "─".repeat(45)
        ));
        if st.namespaces.is_empty() {
            out.push_str("  (none)\n");
        } else {
            let mut names: Vec<&String> = st.namespaces.keys().collect();
            names.sort();
            for name in names {
                let ns = &st.namespaces[name];
                out.push_str(&format!(
                    "  {} → {}\n    functions: {}  enums: {}  local_imports: {}\n",
                    name,
                    ns.file_path,
                    ns.functions.len(),
                    ns.enums.len(),
                    ns.local_imports.len()
                ));
                if verbose {
                    if !ns.functions.is_empty() {
                        let mut fn_names: Vec<&String> = ns.functions.keys().collect();
                        fn_names.sort();
                        out.push_str(&format!(
                            "    funcs:  {}\n",
                            fn_names.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
                        ));
                    }
                    if !ns.enums.is_empty() {
                        let mut en_names: Vec<&String> = ns.enums.keys().collect();
                        en_names.sort();
                        out.push_str(&format!(
                            "    enums:  {}\n",
                            en_names.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
                        ));
                    }
                }
            }
        }
        out.push('\n');
    }

    // ── BUILTIN STATICS ───────────────────────────────────────────────────────
    if show_all || section == "BUILTINS" {
        out.push_str(&format!(
            "── BUILTIN STATICS ({}) {}\n",
            st.builtin_static_objects.len(),
            "─".repeat(39)
        ));
        if st.builtin_static_objects.is_empty() {
            out.push_str(
                "  (none — populate_builtin_objects() may not have been called)\n",
            );
        } else {
            let mut names = st.builtin_static_objects.clone();
            names.sort();
            out.push_str(&format!("  {}\n", names.join(", ")));
        }
        out.push('\n');
    }

    // ── CONFIG ENTRIES ────────────────────────────────────────────────────────
    if (show_all || section == "CONFIG") && !st.configs.is_empty() {
        out.push_str(&format!(
            "── CONFIG ENTRIES ({}) {}\n",
            st.configs.len(),
            "─".repeat(40)
        ));
        let mut keys: Vec<&String> = st.configs.keys().collect();
        keys.sort();
        for key in keys {
            out.push_str(&format!("  {} → {}\n", key, st.configs[key]));
        }
        out.push('\n');
    }

    // ── SUMMARY ───────────────────────────────────────────────────────────────
    out.push_str("=== Summary ===\n");
    let counts = st.get_symbol_counts();
    let mut count_keys: Vec<&String> = counts.keys().collect();
    count_keys.sort();
    for k in count_keys {
        out.push_str(&format!("  {:<28} {}\n", k, counts[k]));
    }

    out
}
