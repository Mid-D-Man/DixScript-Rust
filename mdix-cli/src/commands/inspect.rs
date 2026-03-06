// dixscript-cli/src/commands/inspect.rs

use std::path::PathBuf;
use clap::Args;
use serde::Serialize;
use crate::commands::{handle_error, GlobalOpts};
use crate::output::{json_output, printer, table};
use crate::services::validation;
use dixscript::Runtime::{DixLoader, DixLoadOptions};

#[derive(Args)]
pub struct InspectArgs {
    /// Path to the .dixscript file
    pub file: PathBuf,

    /// Show only section summary
    #[arg(long)]
    pub sections: bool,

    /// List all data keys with types
    #[arg(long)]
    pub keys: bool,
}

#[derive(Serialize)]
struct InspectOutput {
    file_path:      String,
    file_size:      usize,
    sections:       Vec<String>,
    key_count:      usize,
    enum_count:     usize,
    dlm_modules:    Vec<String>,
    version:        String,
    keys:           Option<Vec<KeyEntry>>,
}

#[derive(Serialize)]
struct KeyEntry {
    path:      String,
    value_type: String,
}

pub fn run(args: InspectArgs, global: &GlobalOpts) -> i32 {
    if let Err(e) = validation::validate_file(&args.file, false) {
        return handle_error(&e, global.json);
    }

    let file_size = std::fs::metadata(&args.file)
        .map(|m| m.len() as usize)
        .unwrap_or(0);

    let loader = DixLoader::new();
    let dix_data = match loader.load_text(
        args.file.to_str().unwrap_or(""),
        &DixLoadOptions::new(),
    ) {
        Ok(d)  => d,
        Err(e) => {
            let err = crate::commands::CliError::CompileError(e);
            return handle_error(&err, global.json);
        }
    };

    let version = dix_data
        .config
        .as_ref()
        .and_then(|c| c.get("version"))
        .cloned()
        .unwrap_or_else(|| "unknown".to_string());

    let mut sections = Vec::new();
    if dix_data.config.is_some()   { sections.push("@CONFIG".to_string()); }
    if dix_data.enums.is_some()    { sections.push("@ENUMS".to_string()); }
    if dix_data.dlm.is_some()      { sections.push("@DLM".to_string()); }
    if dix_data.security.is_some() { sections.push("@SECURITY".to_string()); }
    sections.push("@DATA".to_string());

    let data_map  = dix_data.to_hashmap();
    let key_count = data_map.len();

    let enum_count = dix_data
        .enums
        .as_ref()
        .map(|e| e.len())
        .unwrap_or(0);

    let dlm_modules = dix_data.dlm.clone().unwrap_or_default();

    let key_entries: Option<Vec<KeyEntry>> = if args.keys {
        let mut entries: Vec<KeyEntry> = data_map
            .iter()
            .map(|(k, v)| KeyEntry {
                path:       k.clone(),
                value_type: v.type_name().to_string(),
            })
            .collect();
        entries.sort_by(|a, b| a.path.cmp(&b.path));
        Some(entries)
    } else {
        None
    };

    if global.json {
        json_output::print_result(InspectOutput {
            file_path:   args.file.to_string_lossy().to_string(),
            file_size,
            sections,
            key_count,
            enum_count,
            dlm_modules,
            version,
            keys: key_entries,
        });
        return 0;
    }

    if !global.quiet {
        printer::section("File");
        printer::kv("path",    &args.file.to_string_lossy());
        printer::kv("size",    &crate::services::file_io::format_size(file_size));
        printer::kv("version", &version);

        printer::section("Sections");
        for s in &sections {
            printer::info(&format!("  {}", s));
        }

        if !args.sections {
            printer::section("Data");
            printer::kv("keys",    &key_count.to_string());
            printer::kv("enums",   &enum_count.to_string());
            if !dlm_modules.is_empty() {
                printer::kv("DLM modules", &dlm_modules.join(", "));
            }
        }

        if args.keys {
            if let Some(ref entries) = key_entries {
                printer::section("Keys");
                let rows: Vec<Vec<String>> = entries
                    .iter()
                    .map(|e| vec![e.path.clone(), e.value_type.clone()])
                    .collect();
                table::print_table(&["path", "type"], &rows);
            }
        }
    }

    0
                 }
