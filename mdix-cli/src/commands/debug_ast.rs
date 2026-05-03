// mdix-cli/src/commands/debug_ast.rs
//! `mdix debug-ast <file>` — print the parsed AST structure.
//!
//! Runs the full pipeline (config → tokenise → parse → semantic → enhance)
//! and prints the resulting AST in a human-readable debug format.
//!
//! Use this to diagnose:
//!   - Whether enum declarations are parsed correctly.
//!   - Whether QuickFunc scope lists are attached.
//!   - Whether @DATA entries are classified as SimpleProperty / TableProperty /
//!     GroupArray / ObjectProperty correctly.
//!   - Whether the AST position info is sane (for hover/folding debugging).

use std::path::PathBuf;
use clap::Args;
use dixscript::Compiler::Core::Config::ConfigSectionHandler;
use dixscript::Compiler::Core::Tokenizer::Tokenizer;
use dixscript::Compiler::Core::{GeneralParser, GeneralSemanticAnalyzer, GeneralAstEnhancer};
use dixscript::ErrorManager::ErrorManager;
use crate::commands::{CliError, GlobalOpts};
use crate::services::file_io;

#[derive(Args)]
pub struct DebugAstArgs {
    /// Path to the .mdix file
    pub file: PathBuf,

    /// Write output to this file instead of stdout
    #[arg(short, long)]
    pub output: Option<String>,

    /// Show AST node positions (line:col)
    #[arg(long, default_value = "true")]
    pub positions: bool,

    /// Run semantic analysis and enhancement before printing
    #[arg(long, default_value = "true")]
    pub enhanced: bool,

    /// Which section to show: CONFIG, IMPORTS, DLM, ENUMS, QUICKFUNCS, DATA, SECURITY, ALL
    #[arg(long, default_value = "ALL")]
    pub section: String,
}

pub fn run(args: DebugAstArgs, _global: &GlobalOpts) -> i32 {
    let source = match file_io::read_file(&args.file) {
        Ok(s)  => s,
        Err(e) => { eprintln!("Error: {}", e); return 2; }
    };

    let error_manager = ErrorManager::get_shared_instance();
    error_manager.clear_errors();

    let mut config_handler = ConfigSectionHandler::new(None);
    let config_result = config_handler.process_config_section(&source);
    let mut settings = config_result.operational_settings.clone();
    settings.source_file_path = Some(args.file.to_string_lossy().to_string());

    let tokenizer = Tokenizer::new(&config_result.cleaned_input_string, &settings);
    let tok_result = tokenizer.tokenize();

    let parser = match GeneralParser::new(
        tok_result.tokens,
        &config_result.config_section,
        &settings,
    ) {
        Ok(p)  => p,
        Err(e) => { eprintln!("Parser error: {}", e); return 1; }
    };

    let raw_ast = match parser.parse() {
        Ok(a)  => a,
        Err(e) => { eprintln!("Parse error: {}", e); return 1; }
    };

    let ast = if args.enhanced {
        let analyzer = GeneralSemanticAnalyzer::new(&raw_ast, &settings);
        let sem = analyzer.analyze();
        if !sem.is_success {
            eprintln!("⚠ Semantic errors (showing AST anyway):");
            for e in &sem.errors {
                eprintln!("  [{}] {}", e.error_id, e.message);
            }
        }
        let enhancer = GeneralAstEnhancer::new(&settings);
        let enh = enhancer.enhance(&raw_ast, Some(&sem));
        enh.enhanced_ast
    } else {
        raw_ast
    };

    let section_upper = args.section.to_uppercase();

    let mut out = String::new();

    out.push_str(&format!("=== DixScript AST: {} ===\n", args.file.display()));
    out.push_str(&format!("Enhanced: {}\n\n", args.enhanced));

    let show_all = section_upper == "ALL";

    if (show_all || section_upper == "CONFIG") {
        if let Some(ref config) = ast.config {
            out.push_str("@CONFIG\n");
            for entry in &config.entries {
                let pos = if args.positions && entry.position.is_valid() {
                    format!(" @L{}:C{}", entry.position.line, entry.position.column)
                } else { String::new() };
                out.push_str(&format!("  {} -> {:?}{}\n", entry.key, entry.value, pos));
            }
            out.push('\n');
        }
    }

    if (show_all || section_upper == "ENUMS") {
        if let Some(ref enums) = ast.enums {
            out.push_str(&format!("@ENUMS ({} declarations)\n", enums.enums.len()));
            for decl in &enums.enums {
                let pos = fmt_pos(args.positions, decl.position);
                out.push_str(&format!("  enum {}{} {{\n", decl.name, pos));
                for field in &decl.fields {
                    let fpos = fmt_pos(args.positions, field.position);
                    let val  = field.value.map(|v| format!(" = {}", v)).unwrap_or_default();
                    out.push_str(&format!("    {}{}{}\n", field.name, val, fpos));
                }
                out.push_str("  }\n");
            }
            out.push('\n');
        }
    }

    if (show_all || section_upper == "QUICKFUNCS") {
        if let Some(ref qf) = ast.quick_functions {
            out.push_str(&format!("@QUICKFUNCS ({} functions)\n", qf.functions.len()));
            for func in &qf.functions {
                let pos    = fmt_pos(args.positions, func.position);
                let ret    = func.return_type.map(|t| format!("<{}>", t)).unwrap_or_default();
                let scopes = func.scope_list.as_ref()
                    .map(|s| format!(" => {}", s.join(",")))
                    .unwrap_or_default();
                let params: Vec<String> = func.parameters.iter().map(|p| {
                    let t = p.data_type.map(|dt| format!("<{}>", dt)).unwrap_or_default();
                    let ppos = fmt_pos(args.positions, p.position);
                    format!("{}{}{}", p.name, t, ppos)
                }).collect();
                out.push_str(&format!(
                    "  ~{}{}{}({}) body:{} stmts{}\n",
                    func.name, ret, scopes, params.join(", "),
                    func.body.len(), pos
                ));
            }
            out.push('\n');
        }
    }

    if (show_all || section_upper == "DATA") {
        if let Some(ref data) = ast.data {
            out.push_str(&format!("@DATA ({} entries)\n", data.entries.len()));
            for entry in &data.entries {
                match entry {
                    dixscript::Compiler::AST::DataEntry::SimpleProperty { name, data_type, value, position } => {
                        let pos = fmt_pos(args.positions, *position);
                        let dt  = data_type.map(|t| format!("<{}>", t)).unwrap_or_default();
                        out.push_str(&format!(
                            "  SimpleProperty: {}{} = {}{}\n",
                            name, dt,
                            truncate(&format!("{}", value), 60),
                            pos
                        ));
                    }
                    dixscript::Compiler::AST::DataEntry::TableProperty { path, properties, position } => {
                        let pos = fmt_pos(args.positions, *position);
                        out.push_str(&format!(
                            "  TableProperty: {}:{} props{}\n",
                            path.segments.join("."), properties.len(), pos
                        ));
                        for prop in properties {
                            let ppos = fmt_pos(args.positions, prop.position);
                            let dt   = prop.data_type.map(|t| format!("<{}>", t)).unwrap_or_default();
                            out.push_str(&format!(
                                "    .{}{} = {}{}\n",
                                prop.name, dt,
                                truncate(&format!("{}", prop.value), 40),
                                ppos
                            ));
                        }
                    }
                    dixscript::Compiler::AST::DataEntry::GroupArray { path, items, position } => {
                        let pos = fmt_pos(args.positions, *position);
                        out.push_str(&format!(
                            "  GroupArray: {}:: [{} items]{}\n",
                            path.segments.join("."), items.len(), pos
                        ));
                        for (i, item) in items.iter().enumerate().take(5) {
                            out.push_str(&format!(
                                "    [{}] {}\n", i,
                                truncate(&format!("{}", item), 60)
                            ));
                        }
                        if items.len() > 5 {
                            out.push_str(&format!("    ... ({} more)\n", items.len() - 5));
                        }
                    }
                    dixscript::Compiler::AST::DataEntry::ObjectProperty { name, data_type, object, position } => {
                        let pos = fmt_pos(args.positions, *position);
                        let dt  = data_type.map(|t| format!("<{}>", t)).unwrap_or_default();
                        out.push_str(&format!(
                            "  ObjectProperty: {}{} = {}{}\n",
                            name, dt,
                            truncate(&format!("{}", object), 60),
                            pos
                        ));
                    }
                }
            }
            out.push('\n');
        }
    }

    if (show_all || section_upper == "DLM") {
        if let Some(ref dlm) = ast.dlm {
            out.push_str(&format!("@DLM ({} modules)\n", dlm.modules.len()));
            for m in &dlm.modules {
                let sub = m.subtype.map(|s| format!(".{}", s)).unwrap_or_default();
                out.push_str(&format!("  {}{}\n", m.module_type, sub));
            }
            out.push('\n');
        }
    }

    if (show_all || section_upper == "SECURITY") {
        if let Some(ref sec) = ast.security {
            out.push_str(&format!("@SECURITY ({} entries)\n", sec.entries.len()));
            for entry in &sec.entries {
                out.push_str(&format!("  {} -> {{ {} fields }}\n",
                    entry.block_key, entry.fields.len()));
            }
            out.push('\n');
        }
    }

    // Error summary.
    let errors = error_manager.get_all_errors_flat();
    if !errors.is_empty() {
        out.push_str(&format!("=== Errors ({}) ===\n", errors.len()));
        for err in errors.iter().take(20) {
            out.push_str(&format!("  {:?}\n", err));
        }
    }

    match args.output {
        Some(ref path) => {
            if let Err(e) = std::fs::write(path, &out) {
                eprintln!("Failed to write: {}", e);
                return 1;
            }
            println!("AST debug written to: {}", path);
        }
        None => print!("{}", out),
    }

    0
}

fn fmt_pos(show: bool, pos: dixscript::Compiler::AST::Position) -> String {
    if show && pos.is_valid() {
        format!(" @L{}:C{}", pos.line, pos.column)
    } else {
        String::new()
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max { s.to_string() }
    else { format!("{}…", &s[..max]) }
}
