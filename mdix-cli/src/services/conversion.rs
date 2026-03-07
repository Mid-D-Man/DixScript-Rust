// mdix-cli/src/services/conversion.rs

use std::path::Path;
use std::time::Instant;
use dixscript::Runtime::{DixConverter, DixFormatOptions, DixLoader, DixLoadOptions};
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

fn mdix_to_json(path: &Path, pretty: bool) -> Result<String, CliError> {
    let loader   = DixLoader::new();
    let dix_data = loader
        .load_text(path.to_str().unwrap_or(""), &DixLoadOptions::new())
        .map_err(CliError::ConversionError)?;

    let map = dix_data.to_hashmap();

    if pretty {
        serde_json::to_string_pretty(&map)
            .map_err(|e| CliError::ConversionError(e.to_string()))
    } else {
        serde_json::to_string(&map)
            .map_err(|e| CliError::ConversionError(e.to_string()))
    }
}

fn json_to_mdix(path: &Path, pretty: bool) -> Result<String, CliError> {
    let content = file_io::read_file(path)?;
    let map: std::collections::HashMap<String, serde_json::Value> =
        serde_json::from_str(&content)
            .map_err(|e| CliError::ConversionError(format!("Invalid JSON: {}", e)))?;

    let dix_map: std::collections::HashMap<String, dixscript::Runtime::DixValue> = map
        .into_iter()
        .map(|(k, v)| (k, json_value_to_dix(v)))
        .collect();

    let converter = DixConverter::new();
    let ast = converter
        .from_hashmap(dix_map)
        .map_err(CliError::ConversionError)?;

    let fmt_opts = if pretty {
        DixFormatOptions::pretty()
    } else {
        DixFormatOptions::new()
    };

    converter
        .to_mdix(&ast, Some(&fmt_opts))
        .map_err(CliError::ConversionError)
}

fn toml_to_mdix(path: &Path, pretty: bool) -> Result<String, CliError> {
    let content = file_io::read_file(path)?;
    let value: toml::Value = toml::from_str(&content)
        .map_err(|e| CliError::ConversionError(format!("Invalid TOML: {}", e)))?;

    let json_str = serde_json::to_string(&value)
        .map_err(|e| CliError::ConversionError(e.to_string()))?;

    let tmp = tempfile_from_json(&json_str)?;
    let result = json_to_mdix(&tmp, pretty);

    // Best-effort cleanup of the temp file.
    let _ = std::fs::remove_file(&tmp);
    result
}

fn mdix_to_toml(path: &Path) -> Result<String, CliError> {
    let json_str = mdix_to_json(path, false)?;
    let value: toml::Value = serde_json::from_str(&json_str)
        .map_err(|e| CliError::ConversionError(e.to_string()))?;
    toml::to_string_pretty(&value)
        .map_err(|e| CliError::ConversionError(e.to_string()))
}

fn tempfile_from_json(json: &str) -> Result<std::path::PathBuf, CliError> {
    let tmp = std::env::temp_dir().join(format!(
        "mdix_conv_{}.json",
        uuid::Uuid::new_v4()
    ));
    std::fs::write(&tmp, json).map_err(CliError::IoError)?;
    Ok(tmp)
}

// ── serde_json → DixValue ─────────────────────────────────────────────────────

fn json_value_to_dix(v: serde_json::Value) -> dixscript::Runtime::DixValue {
    use dixscript::Runtime::DixValue;
    use serde_json::Value;

    match v {
        Value::Null        => DixValue::Null,
        Value::Bool(b)     => DixValue::Bool(b),
        Value::Number(n)   => {
            if let Some(i) = n.as_i64() {
                DixValue::Int(i as i32)
            } else {
                DixValue::Double(n.as_f64().unwrap_or(0.0))
            }
        }
        Value::String(s)   => DixValue::String(s),
        Value::Array(arr)  => DixValue::Array(arr.into_iter().map(json_value_to_dix).collect()),
        Value::Object(obj) => DixValue::Object(
            obj.into_iter().map(|(k, v)| (k, json_value_to_dix(v))).collect()
        ),
    }
}
