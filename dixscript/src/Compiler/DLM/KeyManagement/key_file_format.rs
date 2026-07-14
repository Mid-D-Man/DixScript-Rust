//! `.mdix.key` file format — writer and parser.
//!
//! The format mirrors the DixScript section syntax so key files are
//! human-readable and consistent with the rest of the toolchain.
//! Commas between entries are always emitted (valid per the grammar) and
//! stripped on parse. String values are double-quoted; numeric values are bare.
//!
//! ## Section layout
//! ```text
//! @KEY_CONFIG( ... )
//! @KEY_PIPELINE( ... )
//! @KEY_ENCRYPTION( ... )   // optional
//! @KEY_COMPRESSION( ... )  // optional
//! @KEY_FILE_INFO( ... )
//! ```

use super::key_file_data::*;
use std::collections::HashMap;

// Section keyword constants — matched case-sensitively in our own output.
const SEC_CONFIG:      &str = "@KEY_CONFIG";
const SEC_PIPELINE:    &str = "@KEY_PIPELINE";
const SEC_ENCRYPTION:  &str = "@KEY_ENCRYPTION";
const SEC_COMPRESSION: &str = "@KEY_COMPRESSION";
const SEC_FILE_INFO:   &str = "@KEY_FILE_INFO";

// ─────────────────────────────────────────────────────────────────────────────
// Writer
// ─────────────────────────────────────────────────────────────────────────────

/// Serialises a [`KeyFileData`] to `.mdix` text.
pub struct MdixKeyWriter;

impl MdixKeyWriter {
    /// Produce the complete `.mdix.key` file content.
    pub fn write(data: &KeyFileData) -> String {
        // Estimate: ~600 base + ~500 encryption-with-KDF + ~200 compression
        let mut out = String::with_capacity(1400);

        // File header comments
        out.push_str("// DixScript Key File v1.0.0\n");
        out.push_str(&format!(
            "// Generated: {}\n",
            data.config.generated.format("%Y-%m-%dT%H:%M:%SZ"),
        ));
        if let Some(ref src) = data.config.source_file {
            out.push_str(&format!("// Source: {}\n", src));
        }
        out.push('\n');

        // @KEY_CONFIG
        out.push_str(SEC_CONFIG);
        out.push_str("(\n");
        Self::str_entry(&mut out, "version",     &data.config.version);
        Self::str_entry(&mut out, "key_type",    &data.config.key_type);
        Self::str_entry(&mut out, "generated",   &data.config.generated.format("%Y-%m-%dT%H:%M:%SZ").to_string());
        if let Some(ref src) = data.config.source_file {
            Self::str_entry(&mut out, "source_file", src);
        }
        out.push_str(")\n\n");

        // @KEY_PIPELINE
        out.push_str(SEC_PIPELINE);
        out.push_str("(\n");
        Self::str_entry(&mut out, "modules",  &data.pipeline.modules_used.join(","));
        Self::str_entry(&mut out, "reversal", &data.pipeline.reversal_order.join(","));
        out.push_str(")\n\n");

        // @KEY_ENCRYPTION (optional)
        if let Some(ref enc) = data.key_data.encryption {
            out.push_str(SEC_ENCRYPTION);
            out.push_str("(\n");
            Self::str_entry(&mut out,  "algorithm",      &enc.algorithm);
            Self::uint_entry(&mut out, "key_length",     enc.key_length);
            Self::str_entry(&mut out,  "iv",             &enc.iv);
            Self::str_entry(&mut out,  "security_level", &enc.security_level);
            if let Some(ref key_data) = enc.key_data {
                Self::str_entry(&mut out, "key_data", key_data);
            }
            if let Some(ref kdf) = enc.kdf {
                Self::str_entry(&mut out,  "kdf_algorithm",  &kdf.algorithm);
                Self::str_entry(&mut out,  "kdf_version",    &kdf.kdf_version);
                Self::u32_entry(&mut out,  "kdf_memory",     kdf.memory);
                Self::u32_entry(&mut out,  "kdf_iterations", kdf.iterations);
                Self::u32_entry(&mut out,  "kdf_parallelism",kdf.parallelism);
                Self::str_entry(&mut out,  "salt",           &kdf.salt);
                Self::uint_entry(&mut out, "salt_length",    kdf.salt_length);
            }
            out.push_str(")\n\n");
        }

        // @KEY_COMPRESSION (optional)
        if let Some(ref comp) = data.key_data.compression {
            out.push_str(SEC_COMPRESSION);
            out.push_str("(\n");
            Self::str_entry(&mut out,  "algorithm",       &comp.algorithm);
            Self::uint_entry(&mut out, "original_size",   comp.original_size);
            Self::uint_entry(&mut out, "compressed_size", comp.compressed_size);
            if let Some(ref level) = comp.compression_level {
                Self::str_entry(&mut out, "compression_level", level);
            }
            out.push_str(")\n\n");
        }

        // @KEY_FILE_INFO
        out.push_str(SEC_FILE_INFO);
        out.push_str("(\n");
        Self::uint_entry(&mut out, "original_size",   data.file_info.original_size);
        Self::uint_entry(&mut out, "compressed_size", data.file_info.compressed_size);
        Self::uint_entry(&mut out, "encrypted_size",  data.file_info.encrypted_size);
        Self::str_entry(&mut out,  "created",
            &data.file_info.created.format("%Y-%m-%dT%H:%M:%SZ").to_string());
        if let Some(ref src) = data.file_info.source_file {
            Self::str_entry(&mut out, "source_file", src);
        }
        if let Some(ref out_file) = data.file_info.output_file {
            Self::str_entry(&mut out, "output_file", out_file);
        }
        out.push_str(")\n");

        out
    }

    // ── Private helpers ───────────────────────────────────────────────────────

    #[inline]
    fn str_entry(out: &mut String, key: &str, value: &str) {
        out.push_str("  ");
        out.push_str(key);
        out.push_str(" -> \"");
        out.push_str(value);
        out.push_str("\",\n");
    }

    #[inline]
    fn uint_entry(out: &mut String, key: &str, value: usize) {
        out.push_str("  ");
        out.push_str(key);
        out.push_str(" -> ");
        // avoid format! allocation on hot path
        let buf = value.to_string();
        out.push_str(&buf);
        out.push_str(",\n");
    }

    #[inline]
    fn u32_entry(out: &mut String, key: &str, value: u32) {
        Self::uint_entry(out, key, value as usize);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Parser
// ─────────────────────────────────────────────────────────────────────────────

/// Deserialises `.mdix.key` text back to [`KeyFileData`].
pub struct MdixKeyParser;

impl MdixKeyParser {
    /// Parse the full file content.
    pub fn parse(input: &str) -> Result<KeyFileData, String> {
        let mut data = KeyFileData::new();

        // @KEY_CONFIG (required)
        let config_content = Self::extract_section(input, SEC_CONFIG)
            .ok_or("Missing @KEY_CONFIG section in key file")?;
        let config_entries = Self::parse_entries(&config_content);

        data.config.version  = Self::req_string(&config_entries, "version",  SEC_CONFIG)?;
        data.config.key_type = Self::req_string(&config_entries, "key_type", SEC_CONFIG)?;

        if let Some(gen) = Self::opt_string(&config_entries, "generated") {
            if let Ok(dt) = gen.parse::<chrono::DateTime<chrono::Utc>>() {
                data.config.generated = dt;
            }
        }
        data.config.source_file = Self::opt_string(&config_entries, "source_file");

        // @KEY_PIPELINE (required)
        let pipeline_content = Self::extract_section(input, SEC_PIPELINE)
            .ok_or("Missing @KEY_PIPELINE section in key file")?;
        let pipeline_entries = Self::parse_entries(&pipeline_content);

        if let Some(modules) = Self::opt_string(&pipeline_entries, "modules") {
            data.pipeline.modules_used = Self::split_csv(&modules);
        }
        if let Some(reversal) = Self::opt_string(&pipeline_entries, "reversal") {
            data.pipeline.reversal_order = Self::split_csv(&reversal);
        }

        // @KEY_ENCRYPTION (optional)
        if let Some(enc_content) = Self::extract_section(input, SEC_ENCRYPTION) {
            let enc_entries = Self::parse_entries(&enc_content);

            let algorithm      = Self::req_string(&enc_entries, "algorithm",      SEC_ENCRYPTION)?;
            let iv             = Self::opt_string(&enc_entries, "iv").unwrap_or_default();
            let security_level = Self::opt_string(&enc_entries, "security_level").unwrap_or_else(|| "HIGH".to_string());
            let key_length     = Self::opt_usize(&enc_entries, "key_length").unwrap_or(32);

            let kdf = if enc_entries.contains_key("kdf_algorithm") {
                Some(KDFParameters {
                    algorithm:   Self::opt_string(&enc_entries, "kdf_algorithm").unwrap_or_else(|| "argon2id".to_string()),
                    kdf_version: Self::opt_string(&enc_entries, "kdf_version").unwrap_or_else(|| "1.3".to_string()),
                    memory:      Self::opt_u32(&enc_entries, "kdf_memory").unwrap_or(65536),
                    iterations:  Self::opt_u32(&enc_entries, "kdf_iterations").unwrap_or(3),
                    parallelism: Self::opt_u32(&enc_entries, "kdf_parallelism").unwrap_or(4),
                    salt:        Self::opt_string(&enc_entries, "salt").unwrap_or_default(),
                    salt_length: Self::opt_usize(&enc_entries, "salt_length").unwrap_or(32),
                })
            } else {
                None
            };

            data.key_data.encryption = Some(EncryptionKeyData {
                algorithm,
                key_length,
                security_level,
                key_data: Self::opt_string(&enc_entries, "key_data"),
                iv,
                kdf,
            });
        }

        // @KEY_COMPRESSION (optional)
        if let Some(comp_content) = Self::extract_section(input, SEC_COMPRESSION) {
            let comp_entries = Self::parse_entries(&comp_content);

            data.key_data.compression = Some(CompressionKeyData {
                algorithm:         Self::req_string(&comp_entries, "algorithm", SEC_COMPRESSION)?,
                compression_level: Self::opt_string(&comp_entries, "compression_level"),
                original_size:     Self::opt_usize(&comp_entries, "original_size").unwrap_or(0),
                compressed_size:   Self::opt_usize(&comp_entries, "compressed_size").unwrap_or(0),
            });
        }

        // @KEY_FILE_INFO (required)
        let info_content = Self::extract_section(input, SEC_FILE_INFO)
            .ok_or("Missing @KEY_FILE_INFO section in key file")?;
        let info_entries = Self::parse_entries(&info_content);

        data.file_info.original_size   = Self::opt_usize(&info_entries, "original_size").unwrap_or(0);
        data.file_info.compressed_size = Self::opt_usize(&info_entries, "compressed_size").unwrap_or(0);
        data.file_info.encrypted_size  = Self::opt_usize(&info_entries, "encrypted_size").unwrap_or(0);
        data.file_info.source_file     = Self::opt_string(&info_entries, "source_file");
        data.file_info.output_file     = Self::opt_string(&info_entries, "output_file");

        if let Some(created_str) = Self::opt_string(&info_entries, "created") {
            if let Ok(dt) = created_str.parse::<chrono::DateTime<chrono::Utc>>() {
                data.file_info.created = dt;
            }
        }

        Ok(data)
    }

    // ── Section extraction ────────────────────────────────────────────────────

    /// Find `@SECTION_NAME(...)` and return the content between the parens.
    fn extract_section(input: &str, section_keyword: &str) -> Option<String> {
        let start = input.find(section_keyword)?;
        let after_keyword = &input[start + section_keyword.len()..];

        // Skip whitespace to find `(`
        let open_offset = after_keyword.find('(')?;
        let body = &after_keyword[open_offset..]; // starts with '('

        // Walk to matching ')' — track paren depth, skip quoted strings.
        let mut depth: i32     = 0;
        let mut in_string      = false;
        let mut string_char    = '\0';
        let mut close_byte_pos = None;

        for (byte_pos, ch) in body.char_indices() {
            if in_string {
                if ch == string_char {
                    in_string = false;
                }
                continue;
            }
            match ch {
                '"' | '\'' => { in_string = true; string_char = ch; }
                '('        => depth += 1,
                ')'        => {
                    depth -= 1;
                    if depth == 0 {
                        close_byte_pos = Some(byte_pos);
                        break;
                    }
                }
                _ => {}
            }
        }

        let close = close_byte_pos?;
        // Content between `(` and matching `)` — excludes both delimiters.
        Some(body[1..close].to_string())
    }

    // ── Entry parsing ─────────────────────────────────────────────────────────

    /// Parse `key -> value,?` lines into a HashMap.
    ///
    /// String values have their outer double-quotes stripped.
    /// Numeric and boolean values are stored as-is.
    /// Comments and blank lines are skipped.
    fn parse_entries(content: &str) -> HashMap<String, String> {
        // Count lines to pre-allocate
        let line_count = content.lines().count();
        let mut map = HashMap::with_capacity(line_count.max(4));

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with("//") {
                continue;
            }
            let Some(arrow_pos) = line.find(" -> ") else { continue; };

            let key = line[..arrow_pos].trim();
            if key.is_empty() {
                continue;
            }

            let raw_value = line[arrow_pos + 4..].trim();
            // Remove optional trailing comma
            let raw_value = raw_value.strip_suffix(',').unwrap_or(raw_value).trim_end();
            // Strip surrounding double quotes for string values
            let value = if raw_value.len() >= 2
                && raw_value.starts_with('"')
                && raw_value.ends_with('"')
            {
                &raw_value[1..raw_value.len() - 1]
            } else {
                raw_value
            };

            map.insert(key.to_string(), value.to_string());
        }

        map
    }

    // ── Field accessors ───────────────────────────────────────────────────────

    fn req_string(
        map: &HashMap<String, String>,
        key: &str,
        section: &str,
    ) -> Result<String, String> {
        map.get(key).cloned()
            .ok_or_else(|| format!("Missing required field '{}' in {}", key, section))
    }

    #[inline]
    fn opt_string(map: &HashMap<String, String>, key: &str) -> Option<String> {
        map.get(key).cloned()
    }

    #[inline]
    fn opt_usize(map: &HashMap<String, String>, key: &str) -> Option<usize> {
        map.get(key)?.parse().ok()
    }

    #[inline]
    fn opt_u32(map: &HashMap<String, String>, key: &str) -> Option<u32> {
        map.get(key)?.parse().ok()
    }

    #[inline]
    fn split_csv(s: &str) -> Vec<String> {
        if s.is_empty() {
            return Vec::new();
        }
        s.split(',').map(|p| p.trim().to_string()).collect()
    }
}
