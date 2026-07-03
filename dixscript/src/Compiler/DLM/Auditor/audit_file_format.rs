//! `.mdix.au` file format — writer and parser.
//!
//! ## File layout
//! ```text
//! // DixScript Audit File v1.0.0
//! // Generated: <timestamp>
//! // Source: <file>
//!
//! @AUDIT_CONFIG(
//!   source_file      -> "config.mdix",
//!   max_entries      -> 100,
//!   format           -> "structured",
//!   created          -> "2025-01-01T00:00:00Z"
//! )
//!
//! // ─── Compilation History ──────────────────────────────────────────────────
//!
//! @compilation(
//!   index              -> 1,
//!   compilation_id     -> "abc12345",
//!   timestamp          -> "2025-01-01T00:00:01Z",
//!   source_checksum    -> "sha256:...",
//!   status             -> "SUCCESS",
//!   modules_executed   -> "DCompressor.gzip,DEncryptor.aes256",
//!   execution_time_ms  -> 45.23,
//!   changes_summary    -> "none",
//! )
//!
//! @compilation(
//!   ...
//! )
//! ```
//!
//! `@compilation` is a real section, same as `@AUDIT_CONFIG` — the index and
//! every other field are key-value lines *inside* the parens, not an
//! argument on the header. Entries are appended: each one is its own
//! complete, self-closed `@compilation( ... )` block, so adding a new entry
//! only ever adds text at the end of the file — no existing block, including
//! its closing paren, needs to move.

use super::audit_file_data::{AuditEntryRecord, AuditFileConfig, AuditFileData};
use std::collections::HashMap;

const SEC_AUDIT_CONFIG: &str = "@AUDIT_CONFIG";
const SEC_COMPILATION:  &str = "@compilation";

// ─────────────────────────────────────────────────────────────────────────────
// Writer
// ─────────────────────────────────────────────────────────────────────────────

/// Serialises audit file components to text.
pub struct AuditFileWriter;

impl AuditFileWriter {
    /// Produce the file header: comment block + `@AUDIT_CONFIG` section +
    /// the history separator comment. Written exactly once when creating a
    /// new audit file; entries are then appended individually.
    pub fn write_header(config: &AuditFileConfig) -> String {
        let mut out = String::with_capacity(300);

        // Comment block
        out.push_str("// DixScript Audit File v1.0.0\n");
        out.push_str(&format!(
            "// Generated: {}\n",
            config.created.format("%Y-%m-%dT%H:%M:%SZ"),
        ));
        if !config.source_file.is_empty() {
            out.push_str(&format!("// Source: {}\n", config.source_file));
        }
        out.push('\n');

        // @AUDIT_CONFIG section
        out.push_str(SEC_AUDIT_CONFIG);
        out.push_str("(\n");
        Self::str_entry(&mut out, "source_file", &config.source_file);
        Self::uint_entry(&mut out, "max_entries", config.max_entries);
        Self::str_entry(&mut out, "format", &config.format);
        Self::str_entry(
            &mut out,
            "created",
            &config.created.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        );
        out.push_str(")\n\n");

        // History separator (cosmetic only — not parsed)
        out.push_str(
            "// ─── Compilation History ────────────────────────────────────────────────────\n\n",
        );

        out
    }

    /// Produce a single `@compilation( ... )` block for one compilation
    /// entry — a real section, same shape as `@AUDIT_CONFIG`, not a bare
    /// header with a free-standing body.
    pub fn write_entry(entry: &AuditEntryRecord) -> String {
        let mut out = String::with_capacity(256);

        out.push_str(SEC_COMPILATION);
        out.push_str("(\n");
        Self::uint_entry(&mut out, "index", entry.index);
        Self::str_entry(&mut out, "compilation_id", &entry.compilation_id);
        Self::str_entry(
            &mut out,
            "timestamp",
            &entry.timestamp.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        );
        Self::str_entry(&mut out, "source_checksum", &entry.source_checksum);
        Self::str_entry(&mut out, "status", &entry.status);
        Self::str_entry(
            &mut out,
            "modules_executed",
            &entry.modules_executed.join(","),
        );
        out.push_str(&format!(
            "  execution_time_ms -> {:.2},\n",
            entry.execution_time_ms,
        ));
        Self::str_entry(
            &mut out,
            "changes_summary",
            entry.changes_summary.as_deref().unwrap_or("none"),
        );
        out.push_str(")\n\n");

        out
    }

    // ── Private helpers ───────────────────────────────────────────────────────

    #[inline]
    fn str_entry(out: &mut String, key: &str, value: &str) {
        out.push_str(&format!("  {} -> \"{}\",\n", key, value));
    }

    #[inline]
    fn uint_entry(out: &mut String, key: &str, value: usize) {
        out.push_str(&format!("  {} -> {},\n", key, value));
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Parser
// ─────────────────────────────────────────────────────────────────────────────

/// Deserialises `.mdix.au` text back to [`AuditFileData`].
pub struct AuditFileParser;

impl AuditFileParser {
    /// Parse the full audit file content.
    pub fn parse(input: &str) -> Result<AuditFileData, String> {
        // ── @AUDIT_CONFIG (required, exactly one) ─────────────────────────────
        let config_content = Self::extract_sections(input, SEC_AUDIT_CONFIG)
            .into_iter()
            .next()
            .ok_or("Missing @AUDIT_CONFIG section in audit file")?;

        let cfg_map = Self::parse_entries(&config_content);

        let source_file = Self::opt_string(&cfg_map, "source_file").unwrap_or_default();
        let max_entries  = Self::opt_usize(&cfg_map, "max_entries").unwrap_or(100);
        let format       = Self::opt_string(&cfg_map, "format")
            .unwrap_or_else(|| "structured".to_string());

        let mut config = AuditFileConfig::new(source_file, max_entries);
        config.format  = format;

        if let Some(s) = Self::opt_string(&cfg_map, "created") {
            if let Ok(dt) = s.parse::<chrono::DateTime<chrono::Utc>>() {
                config.created = dt;
            }
        }

        // ── @compilation sections (zero or more) ──────────────────────────────
        let entries = Self::extract_sections(input, SEC_COMPILATION)
            .iter()
            .map(|block| {
                let map = Self::parse_entries(block);

                let mut rec = AuditEntryRecord::new();
                rec.index             = Self::opt_usize(&map, "index").unwrap_or(0);
                rec.compilation_id    = Self::opt_string(&map, "compilation_id").unwrap_or_default();
                rec.source_checksum   = Self::opt_string(&map, "source_checksum").unwrap_or_default();
                rec.status            = Self::opt_string(&map, "status")
                    .unwrap_or_else(|| "UNKNOWN".to_string());
                rec.execution_time_ms = Self::opt_f64(&map, "execution_time_ms").unwrap_or(0.0);
                rec.changes_summary   = Self::opt_string(&map, "changes_summary")
                    .filter(|s| *s != "none" && !s.is_empty());

                if let Some(ts) = Self::opt_string(&map, "timestamp") {
                    if let Ok(dt) = ts.parse::<chrono::DateTime<chrono::Utc>>() {
                        rec.timestamp = dt;
                    }
                }

                if let Some(modules) = Self::opt_string(&map, "modules_executed") {
                    rec.modules_executed = modules
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                }

                rec
            })
            .collect();

        Ok(AuditFileData { config, entries })
    }

    /// Count `@compilation( ... )` blocks in file content.
    ///
    /// Used by [`AuditFileManager`] to decide when to rotate.
    pub fn count_entries(content: &str) -> usize {
        Self::extract_sections(content, SEC_COMPILATION).len()
    }

    // ── Private — section extraction ──────────────────────────────────────────

    /// Find every `keyword( ... )` occurrence in `input` and return the
    /// content between each pair of balanced parens, in order. Paren
    /// depth-tracked and string-aware, so a `)` inside a `"..."` value
    /// doesn't close the section early. Used for both the single
    /// `@AUDIT_CONFIG` section and the repeated `@compilation` sections —
    /// same extraction logic either way, just take one result or all of
    /// them.
    fn extract_sections(input: &str, keyword: &str) -> Vec<String> {
        let mut results   = Vec::new();
        let mut remaining = input;

        while let Some(rel_start) = remaining.find(keyword) {
            let after = &remaining[rel_start + keyword.len()..];
            let Some(open_rel) = after.find('(') else { break; };
            let body = &after[open_rel..]; // starts with '('

            let mut depth       = 0i32;
            let mut in_string   = false;
            let mut string_char = '\0';
            let mut close_pos   = None;

            for (pos, ch) in body.char_indices() {
                if in_string {
                    if ch == string_char { in_string = false; }
                    continue;
                }
                match ch {
                    '"' | '\'' => { in_string = true; string_char = ch; }
                    '('        => depth += 1,
                    ')' => {
                        depth -= 1;
                        if depth == 0 { close_pos = Some(pos); break; }
                    }
                    _ => {}
                }
            }

            // Malformed (unclosed) section — stop scanning rather than loop forever.
            let Some(close) = close_pos else { break; };

            results.push(body[1..close].to_string());

            // Advance past this whole matched section to keep scanning for more.
            let consumed = rel_start + keyword.len() + open_rel + close + 1;
            remaining = &remaining[consumed..];
        }

        results
    }

    // ── Private — key-value line parsing ─────────────────────────────────────

    fn parse_entries(content: &str) -> HashMap<String, String> {
        let mut map = HashMap::new();
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with("//") { continue; }

            let Some(arrow) = line.find(" -> ") else { continue; };
            let key = line[..arrow].trim();
            if key.is_empty() { continue; }

            let raw   = line[arrow + 4..].trim();
            let raw   = raw.strip_suffix(',').unwrap_or(raw).trim_end();
            let value = if raw.len() >= 2 && raw.starts_with('"') && raw.ends_with('"') {
                &raw[1..raw.len() - 1]
            } else {
                raw
            };

            map.insert(key.to_string(), value.to_string());
        }
        map
    }

    // ── Private — typed accessors ─────────────────────────────────────────────

    #[inline]
    fn opt_string(map: &HashMap<String, String>, key: &str) -> Option<String> {
        map.get(key).cloned()
    }

    #[inline]
    fn opt_usize(map: &HashMap<String, String>, key: &str) -> Option<usize> {
        map.get(key)?.parse().ok()
    }

    #[inline]
    fn opt_f64(map: &HashMap<String, String>, key: &str) -> Option<f64> {
        map.get(key)?.parse().ok()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_config() -> AuditFileConfig {
        AuditFileConfig::new("config.mdix".to_string(), 100)
    }

    fn make_entry(index: usize) -> AuditEntryRecord {
        AuditEntryRecord {
            index,
            compilation_id:    format!("id{}", index),
            timestamp:         Utc::now(),
            source_checksum:   format!("sha256:abc{}", index),
            status:            "SUCCESS".to_string(),
            modules_executed:  vec!["DCompressor.gzip".to_string()],
            execution_time_ms: 42.0,
            changes_summary:   None,
        }
    }

    #[test]
    fn roundtrip_header_and_entry() {
        let config = make_config();
        let entry  = make_entry(1);
        let text   = format!(
            "{}{}",
            AuditFileWriter::write_header(&config),
            AuditFileWriter::write_entry(&entry),
        );

        let data = AuditFileParser::parse(&text).unwrap();
        assert_eq!(data.config.source_file, "config.mdix");
        assert_eq!(data.config.max_entries, 100);
        assert_eq!(data.entries.len(), 1);
        assert_eq!(data.entries[0].index, 1);
        assert_eq!(data.entries[0].compilation_id, "id1");
        assert_eq!(data.entries[0].status, "SUCCESS");
    }

    #[test]
    fn count_entries_multiple() {
        let config = make_config();
        let header = AuditFileWriter::write_header(&config);
        let e1 = AuditFileWriter::write_entry(&make_entry(1));
        let e2 = AuditFileWriter::write_entry(&make_entry(2));
        let e3 = AuditFileWriter::write_entry(&make_entry(3));
        let text = format!("{}{}{}{}", header, e1, e2, e3);

        assert_eq!(AuditFileParser::count_entries(&text), 3);
    }

    #[test]
    fn count_entries_empty_file() {
        let config = make_config();
        let header = AuditFileWriter::write_header(&config);
        assert_eq!(AuditFileParser::count_entries(&header), 0);
    }

    #[test]
    fn parse_missing_config_section_errors() {
        let result = AuditFileParser::parse("nothing here");
        assert!(result.is_err());
    }

    #[test]
    fn changes_summary_none_round_trips() {
        let config = make_config();
        let mut entry = make_entry(1);
        entry.changes_summary = None;
        let text = format!(
            "{}{}",
            AuditFileWriter::write_header(&config),
            AuditFileWriter::write_entry(&entry),
        );
        let data = AuditFileParser::parse(&text).unwrap();
        assert!(data.entries[0].changes_summary.is_none());
    }

    #[test]
    fn entry_is_a_real_wrapping_section() {
        let entry = make_entry(7);
        let text  = AuditFileWriter::write_entry(&entry);
        assert!(text.starts_with("@compilation(\n"));
        // The index must be a key-value line inside the parens, not an
        // argument on the header.
        assert!(!text.starts_with("@compilation(7)"));
        assert!(text.contains("index -> 7,"));
        assert!(text.trim_end().ends_with(')'));
    }

    #[test]
    fn closing_paren_inside_string_value_does_not_end_section_early() {
        // A pathological but valid value containing a literal ')' — the
        // depth/string-aware scanner must not stop at it.
        let mut entry = make_entry(1);
        entry.status = "OK (with parens)".to_string();
        let text = AuditFileWriter::write_entry(&entry);
        let blocks = AuditFileParser::extract_sections(&text, "@compilation");
        assert_eq!(blocks.len(), 1);
        assert!(blocks[0].contains("OK (with parens)"));
    }
        }
