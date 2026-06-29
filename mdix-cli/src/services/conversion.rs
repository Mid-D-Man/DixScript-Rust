use std::path::Path;
use std::time::Instant;
use dixscript::Runtime::{DixConverter, DixFormatOptions, DixLoader};
use crate::commands::CliError;
use crate::services::file_io;

#[derive(Debug, Clone, PartialEq)]
pub enum Format {
    Mdix,
    Json,
    Toml,
}

impl Format {
    /// Detect format from a file extension string.
    /// Accepts both "mdix" and "dixscript" as aliases for the native format.
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_lowercase().as_str() {
            "mdix" | "dixscript" => Some(Format::Mdix),
            "json"               => Some(Format::Json),
            "toml"               => Some(Format::Toml),
            _                    => None,
        }
    }

    /// The canonical file extension produced when writing this format.
    pub fn extension(&self) -> &'static str {
        match self {
            Format::Mdix => "mdix",
            Format::Json => "json",
            Format::Toml => "toml",
        }
    }
}

impl std::str::FromStr for Format {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "mdix" | "dixscript" => Ok(Format::Mdix),
            "json"               => Ok(Format::Json),
            "toml"               => Ok(Format::Toml),
            other => Err(format!(
                "Unsupported format '{}'. Use: mdix, json, toml",
                other
            )),
        }
    }
}

pub struct ConvertOpts {
    pub from:   Option<Format>,
    pub to:     Format,
    pub output: Option<String>,
    pub pretty: bool,
}

pub struct ConversionResult {
    pub input_path:  String,
    pub output_path: String,
    pub input_size:  usize,
    pub output_size: usize,
    pub elapsed:     std::time::Duration,
}

/// Detect format from file extension, returning `CliError::UnsupportedFormat`
/// if the extension is unrecognised.
pub fn detect_format(path: &Path) -> Result<Format, CliError> {
    path.extension()
        .and_then(|e| e.to_str())
        .and_then(Format::from_extension)
        .ok_or_else(|| {
            CliError::UnsupportedFormat(format!(
                "Cannot detect format from extension of '{}'. Supported: .mdix, .json, .toml",
                path.display()
            ))
        })
}

/// Convert `path` to the target format specified in `opts`.
pub fn convert_file(path: &Path, opts: &ConvertOpts) -> Result<ConversionResult, CliError> {
    if !path.exists() {
        return Err(CliError::FileNotFound(path.to_path_buf()));
    }

    let from = match opts.from.clone() {
        Some(f) => f,
        None    => detect_format(path)?,
    };

    if from == opts.to {
        return Err(CliError::InvalidArgument(
            "Input and output formats are the same".to_string(),
        ));
    }

    let input_size = std::fs::metadata(path)
        .map(|m| m.len() as usize)
        .unwrap_or(0);

    let t = Instant::now();

    let output_content = match (&from, &opts.to) {
        (Format::Mdix, Format::Json) => mdix_to_json(path, opts.pretty)?,
        (Format::Json, Format::Mdix) => json_to_mdix(path, opts.pretty)?,
        (Format::Toml, Format::Mdix) => toml_to_mdix(path, opts.pretty)?,
        (Format::Mdix, Format::Toml) => mdix_to_toml(path)?,
        (Format::Json, Format::Toml) => json_to_toml(path)?,
        (Format::Toml, Format::Json) => toml_to_json(path, opts.pretty)?,
        (f, t) => {
            return Err(CliError::UnsupportedFormat(format!(
                "Conversion from {:?} to {:?} is not supported",
                f, t
            )))
        }
    };

    let output_path = match &opts.output {
        Some(p) => p.clone(),
        None    => file_io::default_output_path(path, opts.to.extension())
            .to_string_lossy()
            .to_string(),
    };

    let output_size = output_content.len();
    file_io::write_file(Path::new(&output_path), &output_content)?;

    Ok(ConversionResult {
        input_path:  path.to_string_lossy().to_string(),
        output_path,
        input_size,
        output_size,
        elapsed: t.elapsed(),
    })
}

// ── Format converters ─────────────────────────────────────────────────────────

/// FIX (2026-06-29): this used to call `DixConverter::to_json`, which
/// reconstructs nested JSON objects from dotted `@DATA` paths. That broke
/// every multi-segment dotted path (e.g. "crates.midn-ecs",
/// "crates.midn-ecs.src") for any downstream consumer expecting flat
/// dotted-string keys (e.g. mdix-scaffold's generate_structure.py) — and
/// silently dropped data outright whenever one declared path was a prefix
/// of another (a GroupArray at "crates.midn-ecs" followed by a deeper one
/// at "crates.midn-ecs.src": the already-inserted Array at "midn-ecs"
/// can't be converted into an Object to hold "src", so the `if let
/// Value::Object(..)` match silently fails and "src" is dropped with no
/// error). `to_json_flat` reuses the same resolved AST and the
/// already-correct `to_hashmap` flattening (also used by
/// mdix-python's `MdixDatabase::to_json`) — every dotted path is its own
/// independent top-level key, never nested, so no path can collide with
/// another. Still benefits from `compile_to_resolved_ast`'s real `@ENUMS`
/// resolution.
fn mdix_to_json(path: &Path, pretty: bool) -> Result<String, CliError> {
    let loader = DixLoader::new();
    let ast = loader
        .compile_to_resolved_ast(path.to_str().unwrap_or(""))
        .map_err(CliError::ConversionError)?;

    let converter = DixConverter::new();
    converter.to_json_flat(&ast, pretty).map_err(CliError::ConversionError)
}

/// FIX: previously hand-rolled its own JSON -> DixValue conversion
/// (`json_value_to_dix`, below) which silently truncated any `i64` to `i32`
/// instead of widening to `DixValue::Long`. `DixConverter::from_json` is the
/// single correct implementation of this conversion — delegate to it.
fn json_to_mdix(path: &Path, pretty: bool) -> Result<String, CliError> {
    let content   = file_io::read_file(path)?;
    let converter = DixConverter::new();

    let ast = converter
        .from_json(&content)
        .map_err(CliError::ConversionError)?;

    let fmt_opts = if pretty { DixFormatOptions::pretty() } else { DixFormatOptions::new() };

    converter
        .to_mdix(&ast, Some(&fmt_opts))
        .map_err(CliError::ConversionError)
}

/// FIX: previously round-tripped TOML -> JSON (writing a temp file) -> mdix
/// via `json_to_mdix`, inheriting that function's truncation bug plus the
/// extra I/O and an unnecessary intermediate format with its own lossiness
/// (e.g. TOML datetimes flattened to JSON strings then re-parsed). Calling
/// `DixConverter::from_toml` directly removes the temp file, the extra hop,
/// and the bug.
fn toml_to_mdix(path: &Path, pretty: bool) -> Result<String, CliError> {
    let content   = file_io::read_file(path)?;
    let converter = DixConverter::new();

    let ast = converter
        .from_toml(&content)
        .map_err(CliError::ConversionError)?;

    let fmt_opts = if pretty { DixFormatOptions::pretty() } else { DixFormatOptions::new() };

    converter
        .to_mdix(&ast, Some(&fmt_opts))
        .map_err(CliError::ConversionError)
}

/// FIX: previously routed through the broken `mdix_to_json`. Now compiles
/// to the resolved AST directly and serializes with `DixConverter::to_toml`.
fn mdix_to_toml(path: &Path) -> Result<String, CliError> {
    let loader = DixLoader::new();
    let ast = loader
        .compile_to_resolved_ast(path.to_str().unwrap_or(""))
        .map_err(CliError::ConversionError)?;

    let converter = DixConverter::new();
    converter.to_toml(&ast).map_err(CliError::ConversionError)
}

/// JSON → TOML via the AST: `from_json` parses into a `DixScript`, `to_toml`
/// serializes it. No `.mdix` involved on either side.
fn json_to_toml(path: &Path) -> Result<String, CliError> {
    let content   = file_io::read_file(path)?;
    let converter = DixConverter::new();

    let ast = converter
        .from_json(&content)
        .map_err(CliError::ConversionError)?;

    converter.to_toml(&ast).map_err(CliError::ConversionError)
}

/// TOML → JSON via the AST: `from_toml` parses into a `DixScript`, `to_json`
/// serializes it.
fn toml_to_json(path: &Path, pretty: bool) -> Result<String, CliError> {
    let content   = file_io::read_file(path)?;
    let converter = DixConverter::new();

    let ast = converter
        .from_toml(&content)
        .map_err(CliError::ConversionError)?;

    converter.to_json(&ast, pretty).map_err(CliError::ConversionError)
    }
