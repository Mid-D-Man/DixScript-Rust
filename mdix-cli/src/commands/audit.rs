use clap::{Args, Subcommand};
use serde::Serialize;
use crate::commands::{handle_error, GlobalOpts};
use crate::output::{json_output, printer};
use crate::services::audit_service;

#[derive(Args)]
pub struct AuditArgs {
    #[command(subcommand)]
    pub subcommand: AuditSubcommand,
}

#[derive(Subcommand)]
pub enum AuditSubcommand {
    /// Show the audit file's header + entry count + latest compile status
    Info(AuditInfoArgs),
    /// List every recorded compilation entry
    View(AuditViewArgs),
    /// List archived (rotated) audit files sitting next to this one
    Archives(AuditArchivesArgs),
}

#[derive(Args)]
pub struct AuditInfoArgs {
    /// Path to the .mdix.au file (typically "<source>.mdix.au" next to the source)
    pub auditfile: String,
}

#[derive(Args)]
pub struct AuditViewArgs {
    /// Path to the .mdix.au file
    pub auditfile: String,

    /// Only show the last N entries (default: all)
    #[arg(short = 'n', long)]
    pub tail: Option<usize>,
}

#[derive(Args)]
pub struct AuditArchivesArgs {
    /// Path to the (current, live) .mdix.au file
    pub auditfile: String,
}

#[derive(Serialize)]
struct AuditInfoOutput {
    path: String,
    source_file: String,
    format: String,
    max_entries: usize,
    created: String,
    entry_count: usize,
    latest_status: Option<String>,
    latest_timestamp: Option<String>,
}

#[derive(Serialize)]
struct AuditEntryOutput {
    index: usize,
    compilation_id: String,
    timestamp: String,
    status: String,
    modules_executed: Vec<String>,
    execution_time_ms: f64,
    source_checksum: String,
    changes_summary: Option<String>,
}

pub fn run(args: AuditArgs, global: &GlobalOpts) -> i32 {
    match args.subcommand {
        AuditSubcommand::Info(a) => run_info(a, global),
        AuditSubcommand::View(a) => run_view(a, global),
        AuditSubcommand::Archives(a) => run_archives(a, global),
    }
}

fn run_info(args: AuditInfoArgs, global: &GlobalOpts) -> i32 {
    match audit_service::get_summary(&args.auditfile) {
        Ok(s) => {
            if global.json {
                json_output::print_result(AuditInfoOutput {
                    path: s.path,
                    source_file: s.source_file,
                    format: s.format,
                    max_entries: s.max_entries,
                    created: s.created,
                    entry_count: s.entry_count,
                    latest_status: s.latest_status,
                    latest_timestamp: s.latest_timestamp,
                });
                return 0;
            }
            if !global.quiet {
                printer::section("Audit File Info");
                printer::kv("source file", &s.source_file);
                printer::kv("format", &s.format);
                printer::kv("entries", &format!("{} / {} max", s.entry_count, s.max_entries));
                printer::kv("created", &s.created);
                match (&s.latest_status, &s.latest_timestamp) {
                    (Some(status), Some(ts)) => {
                        printer::kv("latest compile", &format!("{status} at {ts}"));
                    }
                    _ => printer::kv("latest compile", "(no entries yet)"),
                }
            }
            0
        }
        Err(e) => handle_error(&e, global.json),
    }
}

fn run_view(args: AuditViewArgs, global: &GlobalOpts) -> i32 {
    match audit_service::get_entries(&args.auditfile, args.tail) {
        Ok(entries) => {
            let rows: Vec<AuditEntryOutput> = entries
                .iter()
                .map(|e| AuditEntryOutput {
                    index: e.index,
                    compilation_id: e.compilation_id.clone(),
                    timestamp: e.timestamp.to_rfc3339(),
                    status: e.status.clone(),
                    modules_executed: e.modules_executed.clone(),
                    execution_time_ms: e.execution_time_ms,
                    source_checksum: e.source_checksum.clone(),
                    changes_summary: e.changes_summary.clone(),
                })
                .collect();

            if global.json {
                json_output::print_result(rows);
                return 0;
            }

            if !global.quiet {
                if rows.is_empty() {
                    printer::info("No entries recorded yet.");
                } else {
                    printer::section(&format!("{} entr{}", rows.len(), if rows.len() == 1 { "y" } else { "ies" }));
                    for r in &rows {
                        let status_line = format!(
                            "#{}  {}  {}  ({:.1}ms)",
                            r.index, r.timestamp, r.status, r.execution_time_ms
                        );
                        match r.status.as_str() {
                            "FAILED" => printer::warning(&status_line),
                            _        => printer::info(&status_line),
                        }
                        printer::kv("  compilation id", &r.compilation_id);
                        printer::kv("  checksum", &r.source_checksum);
                        if !r.modules_executed.is_empty() {
                            printer::kv("  modules", &r.modules_executed.join(", "));
                        }
                        if let Some(summary) = &r.changes_summary {
                            printer::kv("  changes", summary);
                        }
                    }
                }
            }
            0
        }
        Err(e) => handle_error(&e, global.json),
    }
}

fn run_archives(args: AuditArchivesArgs, global: &GlobalOpts) -> i32 {
    let archives = audit_service::find_archives(&args.auditfile);

    if global.json {
        json_output::print_result(serde_json::json!({ "archives": archives }));
        return 0;
    }
    if !global.quiet {
        if archives.is_empty() {
            printer::info("No rotated archive files found next to this audit file.");
        } else {
            printer::section(&format!("{} archived audit file(s)", archives.len()));
            for a in &archives {
                printer::info(a);
            }
        }
    }
    0
}
