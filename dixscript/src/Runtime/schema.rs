//! Schema validation for loaded DixScript databases.
//!
//! Build a [`SchemaBuilder`], call [`DixData::validate_schema`], and inspect
//! the returned [`ValidationReport`].
//!
//! ```rust,ignore
//! use dixscript::Runtime::{SchemaBuilder, ExpectedValueType};
//!
//! let report = data.validate_schema(
//!     SchemaBuilder::new()
//!         .require_string("app_name")
//!         .require_int("port")
//!         .require_with("port", ExpectedValueType::Int, |data| {
//!             let port: i32 = data.get("port")?;
//!             if (1025..=65535).contains(&port) { Ok(()) }
//!             else { Err(format!("port {} out of range 1025–65535", port)) }
//!         })
//!         .optional_bool("debug")
//!         .optional_string("log_file"),
//! );
//!
//! if !report.is_valid() {
//!     eprintln!("{}", report);
//!     // [Missing] 'app_name': expected string (required), got missing
//!     // [InvalidValue] 'port': expected custom validation to pass, got port 80 out of range
//! }
//! ```

use super::dix_data::DixData;
use super::dix_value::DixValue;

// ── Expected type ─────────────────────────────────────────────────────────────

/// The value type a schema field must satisfy.
#[derive(Debug, Clone, PartialEq)]
pub enum ExpectedValueType {
    String,
    Int,
    Float,
    Double,
    Bool,
    Array,
    Object,
    Date,
    Timestamp,
    HexColor,
    Blob,
    Regex,
    Enum,
    /// Accept any value type.
    Any,
}

impl std::fmt::Display for ExpectedValueType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            ExpectedValueType::String    => "string",
            ExpectedValueType::Int       => "int",
            ExpectedValueType::Float     => "float",
            ExpectedValueType::Double    => "double",
            ExpectedValueType::Bool      => "bool",
            ExpectedValueType::Array     => "array",
            ExpectedValueType::Object    => "object",
            ExpectedValueType::Date      => "date",
            ExpectedValueType::Timestamp => "timestamp",
            ExpectedValueType::HexColor  => "hexcolor",
            ExpectedValueType::Blob      => "blob",
            ExpectedValueType::Regex     => "regex",
            ExpectedValueType::Enum      => "enum",
            ExpectedValueType::Any       => "any",
        };
        write!(f, "{}", s)
    }
}

// ── Validation error ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum ValidationErrorKind {
    /// The field is required but absent.
    Missing,
    /// The field is present but has the wrong value type.
    WrongType,
    /// The field passes the type check but fails a custom validator.
    InvalidValue,
}

impl std::fmt::Display for ValidationErrorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidationErrorKind::Missing      => write!(f, "Missing"),
            ValidationErrorKind::WrongType    => write!(f, "WrongType"),
            ValidationErrorKind::InvalidValue => write!(f, "InvalidValue"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ValidationError {
    pub path:     String,
    pub expected: String,
    pub actual:   String,
    pub kind:     ValidationErrorKind,
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "[{}] '{}': expected {}, got {}",
            self.kind, self.path, self.expected, self.actual
        )
    }
}

// ── Validation report ─────────────────────────────────────────────────────────

/// The result of a schema validation pass. Never panics — always returned.
#[derive(Debug)]
pub struct ValidationReport {
    pub errors: Vec<ValidationError>,
}

impl ValidationReport {
    /// `true` when no errors were found.
    #[inline]
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }

    /// Total number of validation errors.
    #[inline]
    pub fn error_count(&self) -> usize {
        self.errors.len()
    }

    /// All errors of a specific kind.
    pub fn errors_of_kind(&self, kind: &ValidationErrorKind) -> Vec<&ValidationError> {
        self.errors.iter().filter(|e| &e.kind == kind).collect()
    }

    /// Paths that had errors, in order.
    pub fn failed_paths(&self) -> Vec<&str> {
        self.errors.iter().map(|e| e.path.as_str()).collect()
    }
}

impl std::fmt::Display for ValidationReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.is_valid() {
            write!(f, "Validation passed.")
        } else {
            write!(
                f,
                "Validation failed with {} error(s):\n{}",
                self.errors.len(),
                self.errors
                    .iter()
                    .map(|e| e.to_string())
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        }
    }
}

// ── SchemaField ───────────────────────────────────────────────────────────────

type ValidatorFn = Box<dyn Fn(&DixData) -> Result<(), String> + Send + Sync>;

/// A single expected field in a schema.
pub struct SchemaField {
    pub path:          String,
    pub required:      bool,
    pub expected_type: ExpectedValueType,
    pub description:   Option<String>,
    validator:         Option<ValidatorFn>,
}

// ── SchemaBuilder ─────────────────────────────────────────────────────────────

/// Fluent builder for schema definitions.
///
/// All `require_*` / `optional_*` calls chain; each adds one field.
/// `with_description` annotates the most recently added field.
/// Call [`validate`] to run the check — the schema can be used multiple times.
///
/// [`validate`]: SchemaBuilder::validate
pub struct SchemaBuilder {
    fields: Vec<SchemaField>,
}

impl SchemaBuilder {
    pub fn new() -> Self {
        SchemaBuilder { fields: Vec::new() }
    }

    // ── required ──────────────────────────────────────────────────────────────

    /// Add a required field with the given type.
    pub fn require(mut self, path: impl Into<String>, expected_type: ExpectedValueType) -> Self {
        self.fields.push(SchemaField {
            path:          path.into(),
            required:      true,
            expected_type,
            description:   None,
            validator:     None,
        });
        self
    }

    /// Add a required field with a type check AND a custom validator.
    ///
    /// The validator runs only when the type check passes.
    pub fn require_with<F>(
        mut self,
        path: impl Into<String>,
        expected_type: ExpectedValueType,
        validator: F,
    ) -> Self
    where
        F: Fn(&DixData) -> Result<(), String> + Send + Sync + 'static,
    {
        self.fields.push(SchemaField {
            path:          path.into(),
            required:      true,
            expected_type,
            description:   None,
            validator:     Some(Box::new(validator)),
        });
        self
    }

    pub fn require_string(self, path: impl Into<String>) -> Self {
        self.require(path, ExpectedValueType::String)
    }

    pub fn require_int(self, path: impl Into<String>) -> Self {
        self.require(path, ExpectedValueType::Int)
    }

    pub fn require_float(self, path: impl Into<String>) -> Self {
        self.require(path, ExpectedValueType::Float)
    }

    pub fn require_double(self, path: impl Into<String>) -> Self {
        self.require(path, ExpectedValueType::Double)
    }

    pub fn require_bool(self, path: impl Into<String>) -> Self {
        self.require(path, ExpectedValueType::Bool)
    }

    pub fn require_array(self, path: impl Into<String>) -> Self {
        self.require(path, ExpectedValueType::Array)
    }

    pub fn require_object(self, path: impl Into<String>) -> Self {
        self.require(path, ExpectedValueType::Object)
    }

    pub fn require_enum(self, path: impl Into<String>) -> Self {
        self.require(path, ExpectedValueType::Enum)
    }

    // ── optional ──────────────────────────────────────────────────────────────

    /// Add an optional field with the given type.
    ///
    /// When the path is absent, no error is reported. When present, the type
    /// must match.
    pub fn optional(mut self, path: impl Into<String>, expected_type: ExpectedValueType) -> Self {
        self.fields.push(SchemaField {
            path:          path.into(),
            required:      false,
            expected_type,
            description:   None,
            validator:     None,
        });
        self
    }

    /// Add an optional field with a type check AND a custom validator.
    pub fn optional_with<F>(
        mut self,
        path: impl Into<String>,
        expected_type: ExpectedValueType,
        validator: F,
    ) -> Self
    where
        F: Fn(&DixData) -> Result<(), String> + Send + Sync + 'static,
    {
        self.fields.push(SchemaField {
            path:          path.into(),
            required:      false,
            expected_type,
            description:   None,
            validator:     Some(Box::new(validator)),
        });
        self
    }

    pub fn optional_string(self, path: impl Into<String>) -> Self {
        self.optional(path, ExpectedValueType::String)
    }

    pub fn optional_int(self, path: impl Into<String>) -> Self {
        self.optional(path, ExpectedValueType::Int)
    }

    pub fn optional_float(self, path: impl Into<String>) -> Self {
        self.optional(path, ExpectedValueType::Float)
    }

    pub fn optional_double(self, path: impl Into<String>) -> Self {
        self.optional(path, ExpectedValueType::Double)
    }

    pub fn optional_bool(self, path: impl Into<String>) -> Self {
        self.optional(path, ExpectedValueType::Bool)
    }

    pub fn optional_array(self, path: impl Into<String>) -> Self {
        self.optional(path, ExpectedValueType::Array)
    }

    pub fn optional_object(self, path: impl Into<String>) -> Self {
        self.optional(path, ExpectedValueType::Object)
    }

    // ── metadata ──────────────────────────────────────────────────────────────

    /// Attach a human-readable description to the most recently added field.
    ///
    /// Descriptions appear in generated documentation but do not affect
    /// validation outcomes.
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        if let Some(last) = self.fields.last_mut() {
            last.description = Some(description.into());
        }
        self
    }

    // ── inspection ────────────────────────────────────────────────────────────

    /// Number of fields defined in this schema.
    pub fn field_count(&self) -> usize {
        self.fields.len()
    }

    /// All defined field paths.
    pub fn paths(&self) -> Vec<&str> {
        self.fields.iter().map(|f| f.path.as_str()).collect()
    }

    // ── validation ────────────────────────────────────────────────────────────

    /// Validate `data` against this schema.
    ///
    /// All errors are collected — validation never short-circuits.
    /// Returns a [`ValidationReport`] with every problem found.
    pub fn validate(&self, data: &DixData) -> ValidationReport {
        let mut errors = Vec::new();

        for field in &self.fields {
            // 1. Presence check.
            if !data.exists(&field.path) {
                if field.required {
                    errors.push(ValidationError {
                        path:     field.path.clone(),
                        expected: format!("{} (required)", field.expected_type),
                        actual:   "missing".to_string(),
                        kind:     ValidationErrorKind::Missing,
                    });
                }
                // Optional and absent → skip remaining checks.
                continue;
            }

            // 2. Type check.
            let value = data.get_value(&field.path).unwrap();
            if !type_matches(&field.expected_type, value) {
                errors.push(ValidationError {
                    path:     field.path.clone(),
                    expected: field.expected_type.to_string(),
                    actual:   value.type_name().to_string(),
                    kind:     ValidationErrorKind::WrongType,
                });
                // Skip custom validator when type already fails.
                continue;
            }

            // 3. Custom validator (optional).
            if let Some(ref validator) = field.validator {
                match validator(data) {
                    Ok(()) => {}
                    Err(msg) => {
                        errors.push(ValidationError {
                            path:     field.path.clone(),
                            expected: "custom validation to pass".to_string(),
                            actual:   msg,
                            kind:     ValidationErrorKind::InvalidValue,
                        });
                    }
                }
            }
        }

        ValidationReport { errors }
    }
}

impl Default for SchemaBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ── DixData extension ─────────────────────────────────────────────────────────

impl DixData {
    /// Validate this database against `schema`.
    ///
    /// Never panics. Inspect [`ValidationReport::is_valid`] to determine
    /// whether validation passed.
    pub fn validate_schema(&self, schema: SchemaBuilder) -> ValidationReport {
        schema.validate(self)
    }
}

// ── Private helpers ───────────────────────────────────────────────────────────

fn type_matches(expected: &ExpectedValueType, actual: &DixValue) -> bool {
    match expected {
        ExpectedValueType::Any => true,
        ExpectedValueType::String => matches!(
            actual,
            DixValue::String(_)
                | DixValue::Date(_)
                | DixValue::Timestamp(_)
                | DixValue::HexColor(_)
                | DixValue::Blob(_)
                | DixValue::Regex(_)
        ),
        ExpectedValueType::Int    => matches!(actual, DixValue::Int(_) | DixValue::Enum { .. }),
        ExpectedValueType::Float  => matches!(actual, DixValue::Float(_)),
        ExpectedValueType::Double => matches!(
            actual,
            DixValue::Double(_) | DixValue::Float(_) | DixValue::Int(_)
        ),
        ExpectedValueType::Bool      => matches!(actual, DixValue::Bool(_)),
        ExpectedValueType::Array     => matches!(actual, DixValue::Array(_)),
        ExpectedValueType::Object    => matches!(actual, DixValue::Object(_)),
        ExpectedValueType::Date      => matches!(actual, DixValue::Date(_)),
        ExpectedValueType::Timestamp => matches!(actual, DixValue::Timestamp(_)),
        ExpectedValueType::HexColor  => matches!(actual, DixValue::HexColor(_)),
        ExpectedValueType::Blob      => matches!(actual, DixValue::Blob(_)),
        ExpectedValueType::Regex     => matches!(actual, DixValue::Regex(_)),
        ExpectedValueType::Enum      => matches!(actual, DixValue::Enum { .. }),
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Runtime::DixDataBuilder;

    fn make_data() -> DixData {
        DixDataBuilder::new()
            .data(|d| {
                d.with_string("app_name", "TestApp");
                d.with_int("port",        8080);
                d.with_bool("debug",      true);
                d.with_double("timeout",  30.0);
                d.with_group_array_builder("tags", |arr| {
                    arr.add_string("web");
                    arr.add_string("api");
                });
            })
            .build()
            .unwrap()
    }

    #[test]
    fn test_all_required_present_passes() {
        let data = make_data();
        let report = data.validate_schema(
            SchemaBuilder::new()
                .require_string("app_name")
                .require_int("port")
                .require_bool("debug"),
        );
        assert!(report.is_valid(), "{}", report);
    }

    #[test]
    fn test_missing_required_fails() {
        let data = make_data();
        let report = data.validate_schema(
            SchemaBuilder::new().require_string("nonexistent"),
        );
        assert!(!report.is_valid());
        assert_eq!(report.error_count(), 1);
        assert_eq!(report.errors[0].kind, ValidationErrorKind::Missing);
    }

    #[test]
    fn test_wrong_type_fails() {
        let data = make_data();
        // port is Int, not String
        let report = data.validate_schema(SchemaBuilder::new().require_string("port"));
        assert!(!report.is_valid());
        assert_eq!(report.errors[0].kind, ValidationErrorKind::WrongType);
    }

    #[test]
    fn test_optional_absent_passes() {
        let data = make_data();
        let report = data.validate_schema(
            SchemaBuilder::new()
                .require_string("app_name")
                .optional_string("nonexistent"),
        );
        assert!(report.is_valid(), "{}", report);
    }

    #[test]
    fn test_optional_present_wrong_type_fails() {
        let data = make_data();
        // port is present but not a string
        let report = data.validate_schema(SchemaBuilder::new().optional_string("port"));
        assert!(!report.is_valid());
        assert_eq!(report.errors[0].kind, ValidationErrorKind::WrongType);
    }

    #[test]
    fn test_custom_validator_passes() {
        let data = make_data();
        let report = data.validate_schema(
            SchemaBuilder::new().require_with("port", ExpectedValueType::Int, |data| {
                let port: i32 = data.get("port")?;
                if port > 1024 { Ok(()) } else { Err("port must be > 1024".into()) }
            }),
        );
        assert!(report.is_valid(), "{}", report);
    }

    #[test]
    fn test_custom_validator_fails() {
        let data = make_data();
        let report = data.validate_schema(
            SchemaBuilder::new().require_with("port", ExpectedValueType::Int, |data| {
                let port: i32 = data.get("port")?;
                if port < 1024 { Ok(()) } else { Err(format!("port {} too high", port)) }
            }),
        );
        assert!(!report.is_valid());
        assert_eq!(report.errors[0].kind, ValidationErrorKind::InvalidValue);
        assert!(report.errors[0].actual.contains("8080"));
    }

    #[test]
    fn test_multiple_errors_all_collected() {
        let data = make_data();
        let report = data.validate_schema(
            SchemaBuilder::new()
                .require_string("missing_a")
                .require_int("missing_b")
                .require_bool("missing_c"),
        );
        assert_eq!(report.error_count(), 3);
        assert!(report.errors.iter().all(|e| e.kind == ValidationErrorKind::Missing));
    }

    #[test]
    fn test_custom_validator_skipped_on_wrong_type() {
        // If type check fails, validator must not run (it would crash on type assumptions)
        let data = make_data();
        let report = data.validate_schema(
            SchemaBuilder::new().require_with("app_name", ExpectedValueType::Int, |data| {
                let _: i32 = data.get("app_name")?; // would fail
                Ok(())
            }),
        );
        // Only one error (WrongType), not two
        assert_eq!(report.error_count(), 1);
        assert_eq!(report.errors[0].kind, ValidationErrorKind::WrongType);
    }

    #[test]
    fn test_require_array() {
        let data = make_data();
        let report = data.validate_schema(SchemaBuilder::new().require_array("tags"));
        assert!(report.is_valid(), "{}", report);
    }

    #[test]
    fn test_errors_of_kind() {
        let data = make_data();
        let report = data.validate_schema(
            SchemaBuilder::new()
                .require_string("missing_a") // Missing
                .require_string("port"),     // WrongType (port is int)
        );
        assert_eq!(report.errors_of_kind(&ValidationErrorKind::Missing).len(), 1);
        assert_eq!(report.errors_of_kind(&ValidationErrorKind::WrongType).len(), 1);
    }

    #[test]
    fn test_failed_paths() {
        let data = make_data();
        let report = data.validate_schema(SchemaBuilder::new().require_string("gone"));
        assert_eq!(report.failed_paths(), vec!["gone"]);
    }

    #[test]
    fn test_schema_reusable() {
        let schema = SchemaBuilder::new()
            .require_string("app_name")
            .require_int("port");
        let data = make_data();
        let r1 = schema.validate(&data);
        let r2 = schema.validate(&data);
        assert!(r1.is_valid());
        assert!(r2.is_valid());
    }

    #[test]
    fn test_display_valid() {
        let r = ValidationReport { errors: vec![] };
        assert_eq!(r.to_string(), "Validation passed.");
    }

    #[test]
    fn test_display_failures_contains_path() {
        let data = make_data();
        let report = data.validate_schema(SchemaBuilder::new().require_string("missing_key"));
        assert!(report.to_string().contains("missing_key"));
        assert!(report.to_string().contains("Missing"));
    }

    #[test]
    fn test_with_description_does_not_affect_validation() {
        let data = make_data();
        let report = data.validate_schema(
            SchemaBuilder::new()
                .require_string("app_name")
                .with_description("The application display name — must not be empty"),
        );
        assert!(report.is_valid(), "{}", report);
    }

    #[test]
    fn test_field_count_and_paths() {
        let schema = SchemaBuilder::new()
            .require_string("a")
            .optional_int("b")
            .require_bool("c");
        assert_eq!(schema.field_count(), 3);
        assert!(schema.paths().contains(&"a"));
        assert!(schema.paths().contains(&"b"));
        assert!(schema.paths().contains(&"c"));
    }
  }
