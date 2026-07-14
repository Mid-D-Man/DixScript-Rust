//! Converts DixScript compiler errors into LSP diagnostics.
//!
//! All DixScript positions are 1-based; LSP expects 0-based.
//! Errors without source positions are placed at the document start (0:0).

use dixscript::ErrorManager::{DixError, ErrorSeverity};
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, NumberOrString, Position, Range};

/// Converts a single `DixError` into an LSP `Diagnostic`.
pub fn to_diagnostic(error: &DixError) -> Diagnostic {
    let (line, col, message, severity, phase) = extract_fields(error);

    let start = lsp_position(line, col);
    let end   = Position::new(start.line, start.character.saturating_add(1));

    Diagnostic {
        range:    Range::new(start, end),
        severity: Some(map_severity(severity)),
        source:   Some(format!("dixscript:{}", phase)),
        message,
        code:     error_code(error),
        ..Default::default()
    }
}

/// Converts a slice of errors into diagnostics.
///
/// Filters out duplicate (line, character, message) triples so the editor
/// does not stack multiple identical squiggles on the same character.
/// `Range` does not implement `Hash`, so we key on the start position fields
/// and the message text instead.
pub fn to_diagnostics(errors: &[DixError]) -> Vec<Diagnostic> {
    // Key: (start_line, start_character, message)
    let mut seen: std::collections::HashSet<(u32, u32, String)> = std::collections::HashSet::new();
    let mut out = Vec::with_capacity(errors.len());

    for error in errors {
        let diag = to_diagnostic(error);
        let key  = (
            diag.range.start.line,
            diag.range.start.character,
            diag.message.clone(),
        );
        if seen.insert(key) {
            out.push(diag);
        }
    }

    out
}

// ── Internal helpers ──────────────────────────────────────────────────────────

/// (line_1based, col_1based, message, severity, phase_label)
type ErrorFields = (usize, usize, String, ErrorSeverity, &'static str);

fn extract_fields(error: &DixError) -> ErrorFields {
    match error {
        DixError::Lexical(e) => (
            e.line,
            e.column,
            with_suggestion(&e.message, e.suggestion.as_deref()),
            e.severity,
            "lexer",
        ),
        DixError::Parse(e) => (
            e.line,
            e.column,
            with_suggestion(&e.message, e.suggestion.as_deref()),
            e.severity,
            "parser",
        ),
        DixError::Semantic(e) => (
            e.line.max(0) as usize,
            e.column.max(0) as usize,
            with_suggestion(&e.message, e.suggestion.as_deref()),
            e.severity,
            "semantic",
        ),
        DixError::ImportsResolution(e) => (
            e.line.max(0) as usize,
            e.column.max(0) as usize,
            with_suggestion(&e.message, e.suggestion.as_deref()),
            e.severity,
            "imports",
        ),
        DixError::AstEnhancement(e) => (
            e.line.max(0) as usize,
            e.column.max(0) as usize,
            with_suggestion(&e.message, e.suggestion.as_deref()),
            e.severity,
            "ast-enhancement",
        ),
        DixError::ValueResolution(e) => (
            e.line.max(0) as usize,
            e.column.max(0) as usize,
            with_suggestion(&e.message, e.suggestion.as_deref()),
            e.severity,
            "value-resolution",
        ),
        // No source position available for the following phases.
        DixError::Dlm(e) => (
            0, 0,
            with_suggestion(&e.message, e.suggestion.as_deref()),
            e.severity,
            "dlm",
        ),
        DixError::BinarySerialization(e) => (
            0, 0,
            with_suggestion(&e.message, e.suggestion.as_deref()),
            e.severity,
            "binary-serialization",
        ),
        DixError::Runtime(e) => (
            e.line.max(0) as usize,
            e.column.max(0) as usize,
            with_suggestion(&e.message, e.suggestion.as_deref()),
            e.severity,
            "runtime",
        ),
        DixError::Config(e) => (
            e.line.max(0) as usize,
            e.column.max(0) as usize,
            with_suggestion(&e.message, e.suggestion.as_deref()),
            e.severity,
            "config",
        ),
        DixError::General(e) => (
            0, 0,
            with_suggestion(&e.message, e.suggestion.as_deref()),
            e.severity,
            "general",
        ),
    }
}

/// Appends the suggestion to the message so it appears in the editor's hover card.
fn with_suggestion(message: &str, suggestion: Option<&str>) -> String {
    match suggestion {
        Some(s) if !s.is_empty() => format!("{}\n\nSuggestion: {}", message, s),
        _                        => message.to_string(),
    }
}

/// Converts a 1-based DixScript position to a 0-based LSP position.
fn lsp_position(line: usize, col: usize) -> Position {
    Position::new(
        line.saturating_sub(1) as u32,
        col.saturating_sub(1)  as u32,
    )
}

fn map_severity(severity: ErrorSeverity) -> DiagnosticSeverity {
    match severity {
        ErrorSeverity::Fatal | ErrorSeverity::Error => DiagnosticSeverity::ERROR,
        ErrorSeverity::Warning                      => DiagnosticSeverity::WARNING,
        ErrorSeverity::Info                         => DiagnosticSeverity::INFORMATION,
    }
}

/// Uses the compiler's own error ID (e.g. "SEM0002") as the diagnostic code.
/// The editor can display this next to the squiggle for quick reference.
fn error_code(error: &DixError) -> Option<NumberOrString> {
    let id: &str = match error {
        DixError::Lexical(e)             => &e.error_id,
        DixError::Parse(e)               => &e.error_id,
        DixError::Semantic(e)            => &e.error_id,
        DixError::ImportsResolution(e)   => &e.error_id,
        DixError::AstEnhancement(e)      => &e.error_id,
        DixError::ValueResolution(e)     => &e.error_id,
        DixError::Dlm(e)                 => &e.error_id,
        DixError::BinarySerialization(e) => &e.error_id,
        DixError::Runtime(e)             => &e.error_id,
        DixError::Config(e)              => &e.error_id,
        DixError::General(e)             => &e.error_id,
    };

    if id.is_empty() {
        None
    } else {
        Some(NumberOrString::String(id.to_string()))
    }
            }
