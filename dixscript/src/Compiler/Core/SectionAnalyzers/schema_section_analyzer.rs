// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/compiler.md, section "Compiler/Core/SectionAnalyzers/schema_section_analyzer.rs"
// ============================================================================
//! Semantic validation of `@SCHEMA` against `@DATA`.
//!
//! Two jobs, in this order:
//!
//! 1. **Descriptor validation.** Each field's `Schema.<Method>(args...)` is
//!    evaluated by calling the `Schema` builtin static object
//!    (`Builtins/Static/schema_object.rs`) — that builtin is the single
//!    source of truth for which methods exist and which arguments each
//!    accepts, so an unknown method or a badly-typed argument surfaces here
//!    as the builtin's own error. The canonical descriptor Object it returns
//!    (`type` / `required` / constraint keys) is then read back into a
//!    `Descriptor`. Duplicate paths are rejected.
//!
//! 2. **Data validation.** Each descriptor is checked against the value
//!    `@DATA` holds at the same dotted path: presence (`required`), type,
//!    numeric `min`/`max`, string `minLength`/`maxLength`, array
//!    `minItems`/`maxItems`, and the enum name for `Schema.Enum`.
//!
//! ## Runs twice, on purpose
//! * From the semantic-analysis phase (`analyze`), against the `@DATA` AST as
//!   parsed. Values that aren't known yet — QuickFunc calls, references,
//!   expressions — classify as `Kind::Deferred` and are **skipped**, not
//!   failed. Plain literals (the common case) are fully checked, so
//!   `mdix validate` and the LSP see real schema errors.
//! * From `DixLoader` after value resolution (`validate_resolved`), against
//!   the resolved AST, where those computed values are now concrete. That is
//!   the only place a schema can constrain a QuickFunc-produced value.
//!
//! ## Numeric type compatibility
//! `Int` accepts an integer; `Long` accepts an integer or long; `Float` and
//! `Double` accept any numeric literal. A bare `1.5` literal is a double, so
//! rejecting it for `Schema.Float` would force an `f` suffix on every value
//! for no benefit. Narrowing is never implied: an `Int` schema rejects a
//! long or a fractional value.
//!
//! An explicit type annotation on the `@DATA` property (`age<long> = 5`)
//! takes precedence over the literal's own kind, since it is what the
//! runtime will actually store.
//!
//! ## What this does not do
//! No nested-object shape validation (`Schema.Object` only checks presence
//! and kind), no wildcard paths, and no runtime validation of data supplied
//! after compilation — this is compile-time only.
//!
//! ## Reading the arena
//! The checks run over the arena form of the sections:
//! [`analyze_doc`](SchemaSectionAnalyzer::analyze_doc) reads a [`DocSchema`], the `@DATA` section
//! as a [`DocData`], and the [`Arena`] both live in. `GeneralSemanticAnalyzer` calls it when it
//! has a `Document`. [`analyze`](SchemaSectionAnalyzer::analyze) and
//! [`validate_resolved`](SchemaSectionAnalyzer::validate_resolved) keep their owned-type
//! signatures for callers that only have a `DixScript`: they copy both sections into a scratch
//! arena and run the same checks, so the entry points cannot disagree. `DixLoader` still calls
//! `validate_resolved` with the tree that value resolution produced, so that path pays for the
//! copy until the resolver reads the arena too.
//!
//! The `Schema` builtin takes owned values, so `build_descriptor` copies the arguments of one
//! field out of the arena before calling it.

use std::collections::HashMap;

use crate::Builtins::Core::{DixType, DixValue};
use crate::Builtins::Resolver::resolve_static_call_with_conversion;
use crate::Compiler::AST::arena::{Arena, DocData, DocDataEntry, DocSchema, DocSchemaField, ValueId, ValueNode};
use crate::Compiler::AST::{DataSection, DataType, Position, SchemaBlock};
use crate::Compiler::Core::{OperationalSettings, ErrorHandlingStrategy};
use crate::Compiler::Utilities::SymbolTable;
use crate::ErrorManager::{ErrorManager, SemanticErrorType, DebugConfig};

use super::{SectionAnalysisResult, SemanticErrorInfo, SemanticWarningInfo};

const WARN_EMPTY_SCHEMA: &str = "SCHEMA_WARN001";

/// Every diagnostic this analyzer can produce.
#[derive(Clone, Copy)]
enum Code {
    InvalidDescriptor,
    DuplicatePath,
    MissingRequired,
    TypeMismatch,
    OutOfRange,
    LengthViolation,
    EnumMismatch,
}

impl Code {
    fn id(self) -> &'static str {
        match self {
            Code::InvalidDescriptor => "SCH001",
            Code::DuplicatePath     => "SCH002",
            Code::MissingRequired   => "SCH003",
            Code::TypeMismatch      => "SCH004",
            Code::OutOfRange        => "SCH005",
            Code::LengthViolation   => "SCH006",
            Code::EnumMismatch      => "SCH007",
        }
    }

    fn type_name(self) -> &'static str {
        match self {
            Code::InvalidDescriptor => "INVALID_DESCRIPTOR",
            Code::DuplicatePath     => "DUPLICATE_PATH",
            Code::MissingRequired   => "MISSING_REQUIRED_FIELD",
            Code::TypeMismatch      => "TYPE_MISMATCH",
            Code::OutOfRange        => "VALUE_OUT_OF_RANGE",
            Code::LengthViolation   => "LENGTH_VIOLATION",
            Code::EnumMismatch      => "ENUM_MISMATCH",
        }
    }

    fn semantic_type(self) -> SemanticErrorType {
        match self {
            Code::InvalidDescriptor => SemanticErrorType::InvalidLiteral,
            Code::DuplicatePath     => SemanticErrorType::DuplicateDefinition,
            Code::MissingRequired   => SemanticErrorType::UndefinedReference,
            Code::TypeMismatch      => SemanticErrorType::TypeMismatch,
            Code::OutOfRange        => SemanticErrorType::InvalidLiteral,
            Code::LengthViolation   => SemanticErrorType::InvalidLiteral,
            Code::EnumMismatch      => SemanticErrorType::InvalidEnumValue,
        }
    }
}

// ═════════════════════════════════════════════════════════════════════════
// Descriptor — the canonical Object the Schema builtin returns, read back
// ═════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone)]
struct Descriptor {
    type_name:  String,
    required:   bool,
    min:        Option<f64>,
    max:        Option<f64>,
    min_length: Option<i64>,
    max_length: Option<i64>,
    min_items:  Option<i64>,
    max_items:  Option<i64>,
    enum_name:  Option<String>,
}

impl Descriptor {
    /// Reads the canonical shape. A constraint key that is absent means "no
    /// constraint" — the builtin omits unsupplied constraints entirely
    /// rather than storing a null.
    fn from_value(value: &DixValue) -> Result<Self, String> {
        if !value.is_object() {
            return Err("descriptor did not evaluate to an object".to_string());
        }
        let obj = value.as_object();

        let type_name = match obj.get("type") {
            Some(t) if t.is_string() => t.as_string(),
            _ => return Err("descriptor is missing its 'type' key".to_string()),
        };

        let required = match obj.get("required") {
            Some(r) if r.get_type() == DixType::Bool => r.as_bool(),
            _ => return Err("descriptor is missing a bool 'required' key".to_string()),
        };

        let as_f64 = |key: &str| obj.get(key).filter(|v| v.is_numeric()).map(|v| v.as_double());
        let as_i64 = |key: &str| obj.get(key).filter(|v| v.is_numeric()).map(|v| v.as_long());

        Ok(Descriptor {
            type_name,
            required,
            min:        as_f64("min"),
            max:        as_f64("max"),
            min_length: as_i64("minLength"),
            max_length: as_i64("maxLength"),
            min_items:  as_i64("minItems"),
            max_items:  as_i64("maxItems"),
            enum_name:  obj.get("enumName").filter(|v| v.is_string()).map(|v| v.as_string()),
        })
    }
}

/// Evaluates a field's `Schema.<Method>(args...)` through the builtin.
///
/// The builtin takes owned values, so the arguments of this one field are copied out of the
/// arena first. They are a handful of literals, not the data section.
fn build_descriptor(field: &DocSchemaField, arena: &Arena) -> Result<Descriptor, String> {
    let arguments: Vec<_> = arena
        .value_list(field.arguments)
        .iter()
        .map(|&id| arena.export_value(id))
        .collect();
    let value = resolve_static_call_with_conversion("Schema", &field.method, &arguments)?;
    Descriptor::from_value(&value)
}

// ═════════════════════════════════════════════════════════════════════════
// Locating a path in @DATA
// ═════════════════════════════════════════════════════════════════════════

enum Lookup<'a> {
    /// A concrete property (top-level, in a table, or inside an object).
    Value { value: ValueId, declared: Option<&'a DataType>, position: Position },
    /// A `path:: a, b, c` group array: no single value holds it.
    Group { len: usize, position: Position },
    /// The path names a table (or a prefix of one) rather than a property.
    Table { position: Position },
    Missing,
}

/// Follows `rest` through nested object properties.
fn walk_value(arena: &Arena, value: ValueId, rest: &[String]) -> Option<ValueId> {
    if rest.is_empty() {
        return Some(value);
    }
    match arena.value(value) {
        ValueNode::Object { properties, .. } => arena
            .prop_list(*properties)
            .iter()
            .find(|p| arena.text(p.key) == rest[0])
            .and_then(|p| walk_value(arena, p.value, &rest[1..])),
        _ => None,
    }
}

/// Resolves a dotted path against every `@DATA` entry shape:
/// `name = v`, `name = { .. }`, `a.b: key = v`, and `a.b:: items`.
fn lookup<'a>(arena: &Arena, data: &'a DocData, segs: &[String]) -> Lookup<'a> {
    if segs.is_empty() {
        return Lookup::Missing;
    }

    let mut table_hit: Option<Position> = None;

    for entry in &data.entries {
        match entry {
            DocDataEntry::SimpleProperty { name, data_type, value, .. } => {
                if *name == segs[0] {
                    if segs.len() == 1 {
                        return Lookup::Value {
                            value: *value,
                            declared: data_type.as_ref(),
                            position: arena.value(*value).position(),
                        };
                    }
                    if let Some(found) = walk_value(arena, *value, &segs[1..]) {
                        return Lookup::Value {
                            value: found,
                            declared: None,
                            position: arena.value(found).position(),
                        };
                    }
                }
            }
            DocDataEntry::ObjectProperty { name, data_type, object, .. } => {
                if *name == segs[0] {
                    if segs.len() == 1 {
                        return Lookup::Value {
                            value: *object,
                            declared: data_type.as_ref(),
                            position: arena.value(*object).position(),
                        };
                    }
                    if let Some(found) = walk_value(arena, *object, &segs[1..]) {
                        return Lookup::Value {
                            value: found,
                            declared: None,
                            position: arena.value(found).position(),
                        };
                    }
                }
            }
            DocDataEntry::TableProperty { path, properties, position } => {
                let p = &path.segments;
                if segs.len() > p.len() && segs[..p.len()] == p[..] {
                    let key = &segs[p.len()];
                    for prop in properties {
                        if prop.name == *key {
                            if segs.len() == p.len() + 1 {
                                return Lookup::Value {
                                    value: prop.value,
                                    declared: prop.data_type.as_ref(),
                                    position: prop.position,
                                };
                            }
                            if let Some(found) = walk_value(arena, prop.value, &segs[p.len() + 1..]) {
                                return Lookup::Value {
                                    value: found,
                                    declared: None,
                                    position: arena.value(found).position(),
                                };
                            }
                        }
                    }
                } else if segs.len() <= p.len() && p[..segs.len()] == segs[..] {
                    table_hit.get_or_insert(*position);
                }
            }
            DocDataEntry::GroupArray { path, items, position } => {
                if path.segments.as_slice() == segs {
                    return Lookup::Group { len: items.len(), position: *position };
                }
                if segs.len() < path.segments.len() && path.segments[..segs.len()] == segs[..] {
                    table_hit.get_or_insert(*position);
                }
            }
        }
    }

    match table_hit {
        Some(position) => Lookup::Table { position },
        None => Lookup::Missing,
    }
}

// ═════════════════════════════════════════════════════════════════════════
// Value classification and compatibility
// ═════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, PartialEq)]
enum Kind {
    Int,
    Long,
    Float,
    Double,
    Str,
    Bool,
    Hex,
    Date,
    Timestamp,
    Array,
    Object,
    Tuple,
    Blob,
    Regex,
    /// `Some(name)` when the enum name is known from the literal.
    Enum(Option<String>),
    Null,
    /// Not knowable yet (QuickFunc call, reference, expression, ...).
    Deferred,
}

impl Kind {
    fn name(&self) -> &'static str {
        match self {
            Kind::Int       => "int",
            Kind::Long      => "long",
            Kind::Float     => "float",
            Kind::Double    => "double",
            Kind::Str       => "string",
            Kind::Bool      => "bool",
            Kind::Hex       => "hex",
            Kind::Date      => "date",
            Kind::Timestamp => "timestamp",
            Kind::Array     => "array",
            Kind::Object    => "object",
            Kind::Tuple     => "tuple",
            Kind::Blob      => "blob",
            Kind::Regex     => "regex",
            Kind::Enum(_)   => "enum",
            Kind::Null      => "null",
            Kind::Deferred  => "unresolved",
        }
    }
}

fn kind_of_value(arena: &Arena, v: &ValueNode) -> Kind {
    match v {
        ValueNode::Integer { .. }            => Kind::Int,
        ValueNode::Long { .. }               => Kind::Long,
        ValueNode::Float { .. }              => Kind::Float,
        ValueNode::Double { .. }             => Kind::Double,
        ValueNode::ScientificNotation { .. } => Kind::Double,
        ValueNode::String { .. }             => Kind::Str,
        ValueNode::InterpolatedString { .. } => Kind::Str,
        ValueNode::Boolean { .. }            => Kind::Bool,
        ValueNode::HexColor { .. }           => Kind::Hex,
        ValueNode::Date { .. }               => Kind::Date,
        ValueNode::Timestamp { .. }          => Kind::Timestamp,
        ValueNode::Array { .. }              => Kind::Array,
        ValueNode::NestedArray { .. }        => Kind::Array,
        ValueNode::Object { .. }             => Kind::Object,
        ValueNode::EnumValue { enum_name, .. } => Kind::Enum(Some(arena.text(*enum_name).to_string())),
        ValueNode::Null { .. }               => Kind::Null,
        // The data parser's prefixed constructors: b(...) blob, t(...) tuple, r(...) regex.
        ValueNode::PrefixedConstructor { prefix, .. } => match arena.text(*prefix) {
            "b" => Kind::Blob,
            "t" => Kind::Tuple,
            "r" => Kind::Regex,
            _   => Kind::Deferred,
        },
        _ => Kind::Deferred,
    }
}

/// The kind an explicit `<type>` annotation promises, where it names one.
fn kind_of_declared(dt: &DataType) -> Option<Kind> {
    match dt {
        DataType::Int       => Some(Kind::Int),
        DataType::Long      => Some(Kind::Long),
        DataType::Float     => Some(Kind::Float),
        DataType::Double    => Some(Kind::Double),
        DataType::String    => Some(Kind::Str),
        DataType::Bool      => Some(Kind::Bool),
        DataType::Hex       => Some(Kind::Hex),
        DataType::Blob      => Some(Kind::Blob),
        DataType::Regex     => Some(Kind::Regex),
        DataType::Object    => Some(Kind::Object),
        DataType::Date      => Some(Kind::Date),
        DataType::Timestamp => Some(Kind::Timestamp),
        DataType::Enum      => Some(Kind::Enum(None)),
        DataType::Array | DataType::TypedArray(_) => Some(Kind::Array),
        DataType::Tuple | DataType::TypedTuple(_) => Some(Kind::Tuple),
        _ => None,
    }
}

fn is_compatible(type_name: &str, kind: &Kind) -> bool {
    if matches!(kind, Kind::Deferred) {
        return true;
    }
    match (type_name, kind) {
        ("int", Kind::Int)                                                     => true,
        ("long", Kind::Int | Kind::Long)                                       => true,
        ("float" | "double", Kind::Int | Kind::Long | Kind::Float | Kind::Double) => true,
        ("string", Kind::Str)                                                  => true,
        ("bool", Kind::Bool)                                                   => true,
        ("hex", Kind::Hex)                                                     => true,
        ("date", Kind::Date)                                                   => true,
        ("timestamp", Kind::Timestamp)                                         => true,
        ("array", Kind::Array)                                                 => true,
        ("object", Kind::Object)                                               => true,
        ("tuple", Kind::Tuple)                                                 => true,
        ("blob", Kind::Blob)                                                   => true,
        ("regex", Kind::Regex)                                                 => true,
        ("enum", Kind::Enum(_))                                                => true,
        _ => false,
    }
}

fn numeric_value(v: &ValueNode) -> Option<f64> {
    match v {
        ValueNode::Integer { value, .. }            => Some(*value as f64),
        ValueNode::Long { value, .. }               => Some(*value as f64),
        ValueNode::Float { value, .. }              => Some(*value as f64),
        ValueNode::Double { value, .. }             => Some(*value),
        ValueNode::ScientificNotation { value, .. } => Some(*value),
        _ => None,
    }
}

/// `Alias.EnumName` (imported) and a bare `EnumName` name the same enum.
fn enum_names_match(actual: &str, expected: &str) -> bool {
    actual == expected
        || actual.ends_with(&format!(".{}", expected))
        || expected.ends_with(&format!(".{}", actual))
}

// ═════════════════════════════════════════════════════════════════════════
// The analyzer
// ═════════════════════════════════════════════════════════════════════════

pub struct SchemaSectionAnalyzer<'a> {
    operational_settings: &'a OperationalSettings,
    error_manager: ErrorManager,
    debug_config: DebugConfig,
}

impl<'a> SchemaSectionAnalyzer<'a> {
    pub fn new(operational_settings: &'a OperationalSettings) -> Self {
        Self::new_with_error_manager(operational_settings, ErrorManager::get_shared_instance())
    }

    pub fn new_with_error_manager(
        operational_settings: &'a OperationalSettings,
        error_manager: ErrorManager,
    ) -> Self {
        SchemaSectionAnalyzer {
            error_manager,
            debug_config: DebugConfig::from_debug_mode(operational_settings.debug_mode),
            operational_settings,
        }
    }

    /// Semantic-phase entry for callers that hold the owned types. `data` is the `@DATA` AST
    /// as parsed; values not yet knowable are skipped (see the module doc comment).
    ///
    /// The sections are copied into a scratch arena and checked by
    /// [`analyze_doc`](Self::analyze_doc), so the diagnostics are the ones `analyze_doc` gives
    /// for the same sections.
    ///
    /// # Panics
    ///
    /// Panics if the sections need more nodes or text than an arena can address (more than
    /// `u32::MAX` of either).
    pub fn analyze(
        &mut self,
        schema: &SchemaBlock,
        data: Option<&DataSection>,
        _symbol_table: &mut SymbolTable,
    ) -> SectionAnalysisResult {
        self.run_owned(schema, data)
    }

    /// Semantic-phase entry for a `Document`. `schema` and `data` must be the `schema` and
    /// `data` fields of the document whose `arena` is passed: the sections hold ids into it.
    ///
    /// # Examples
    ///
    /// ```
    /// use dixscript::Compiler::AST::arena::{Arena, DocData, DocSchema};
    /// use dixscript::Compiler::AST::{
    ///     DataEntry, DataSection, Position, SchemaBlock, SchemaField, TablePath, Value,
    /// };
    /// use dixscript::Compiler::Core::Config::OperationalSettings;
    /// use dixscript::Compiler::Core::SectionAnalyzers::SchemaSectionAnalyzer;
    /// use dixscript::Compiler::Utilities::SymbolTable;
    ///
    /// let pos = Position::new(1, 1);
    /// // `port = 70000` checked against `Schema.Int(true, 1, 65535)`.
    /// let schema = SchemaBlock::new(
    ///     vec![SchemaField::new(
    ///         TablePath::new(vec!["port".to_string()]),
    ///         "Int".to_string(),
    ///         vec![
    ///             Value::Boolean { value: true, position: pos },
    ///             Value::Integer { value: 1, position: pos },
    ///             Value::Integer { value: 65535, position: pos },
    ///         ],
    ///         pos,
    ///     )],
    ///     pos,
    /// );
    /// let data = DataSection::new(
    ///     vec![DataEntry::SimpleProperty {
    ///         name: "port".to_string(),
    ///         data_type: None,
    ///         value: Value::Integer { value: 70000, position: pos },
    ///         position: pos,
    ///     }],
    ///     pos,
    /// );
    ///
    /// let mut arena = Arena::new();
    /// let schema = DocSchema::from_owned(&mut arena, &schema);
    /// let data = DocData::from_owned(&mut arena, &data);
    ///
    /// let settings = OperationalSettings::default();
    /// let mut analyzer = SchemaSectionAnalyzer::new(&settings);
    /// let result = analyzer.analyze_doc(&schema, Some(&data), &arena, &mut SymbolTable::new());
    ///
    /// assert!(!result.is_success);
    /// assert_eq!(result.errors[0].error_id, "SCH005");
    /// ```
    pub fn analyze_doc(
        &mut self,
        schema: &DocSchema,
        data: Option<&DocData>,
        arena: &Arena,
        _symbol_table: &mut SymbolTable,
    ) -> SectionAnalysisResult {
        self.run(schema, data, arena)
    }

    /// Post-resolution entry, called by `DixLoader` once value resolution
    /// has replaced computed values with concrete ones.
    ///
    /// It takes the owned types because value resolution still produces an owned tree. The
    /// sections are copied into a scratch arena and checked by
    /// [`analyze_doc`](Self::analyze_doc), so this costs a copy of the resolved `@DATA`
    /// section that the semantic-phase entry no longer pays.
    ///
    /// # Panics
    ///
    /// Panics if the sections need more nodes or text than an arena can address (more than
    /// `u32::MAX` of either).
    pub fn validate_resolved(
        &mut self,
        schema: &SchemaBlock,
        data: Option<&DataSection>,
    ) -> SectionAnalysisResult {
        self.run_owned(schema, data)
    }

    /// Copies the owned sections into a scratch arena and runs the checks over it.
    fn run_owned(&mut self, schema: &SchemaBlock, data: Option<&DataSection>) -> SectionAnalysisResult {
        let mut arena = Arena::new();
        let doc_schema = DocSchema::from_owned(&mut arena, schema);
        let doc_data = data.map(|d| DocData::from_owned(&mut arena, d));
        self.run(&doc_schema, doc_data.as_ref(), &arena)
    }

    /// Wraps `run_checks` so `is_success` is derived from the error list on
    /// EVERY exit path — the Halt strategy returns early from inside the
    /// checks, and `DixLoader` trusts `is_success` at Stage 9.
    fn run(&mut self, schema: &DocSchema, data: Option<&DocData>, arena: &Arena) -> SectionAnalysisResult {
        let mut result = self.run_checks(schema, data, arena);
        result.is_success = result.errors.is_empty();

        if self.debug_config.is_enabled {
            self.error_manager.log_info(&format!(
                "SCHEMA analysis complete: {} — fields: {}, errors: {}, warnings: {}",
                if result.is_success { "SUCCESS" } else { "FAILURE" },
                schema.fields.len(), result.errors.len(), result.warnings.len(),
            ));
        }
        result
    }

    fn run_checks(&mut self, schema: &DocSchema, data: Option<&DocData>, arena: &Arena) -> SectionAnalysisResult {
        let mut result = SectionAnalysisResult::new("SCHEMA");

        if self.debug_config.is_enabled {
            self.error_manager.log_info(&format!(
                "Analyzing SCHEMA section: {} field(s)", schema.fields.len()
            ));
        }

        if schema.fields.is_empty() {
            self.add_warning(
                &mut result,
                WARN_EMPTY_SCHEMA,
                "@SCHEMA declares no fields, so it constrains nothing",
                Some(schema.position),
            );
        }

        // Pass 1: build every descriptor; reject duplicate paths.
        let mut seen: HashMap<String, Position> = HashMap::new();
        let mut built: Vec<(&DocSchemaField, Descriptor)> = Vec::with_capacity(schema.fields.len());

        for field in &schema.fields {
            let path = field.path_string();

            let first = seen.get(&path).copied();
            if let Some(first_pos) = first {
                self.add_error(
                    &mut result,
                    Code::DuplicatePath,
                    format!(
                        "@SCHEMA path '{}' is declared more than once (first at line {})",
                        path, first_pos.line
                    ),
                    "Keep one descriptor per path".to_string(),
                    Some(field.position),
                );
                if self.should_halt(&result) { return result; }
                continue;
            }
            seen.insert(path.clone(), field.position);

            match build_descriptor(field, arena) {
                Ok(desc) => built.push((field, desc)),
                Err(e) => {
                    self.add_error(
                        &mut result,
                        Code::InvalidDescriptor,
                        format!("Invalid descriptor for '{}': {}", path, e),
                        "Use one of Schema.Int/Long/Float/Double/String/Bool/Array/Enum/Object/Tuple/Date/Timestamp/Hex/Blob/Regex, with 'required' (a bool) as the first argument".to_string(),
                        Some(field.position),
                    );
                    if self.should_halt(&result) { return result; }
                }
            }
        }

        // Pass 2: check @DATA against every descriptor that built.
        for (field, desc) in &built {
            self.check_field(field, desc, data, arena, &mut result);
            if self.should_halt(&result) { return result; }
        }

        result
    }

    fn check_field(
        &mut self,
        field: &DocSchemaField,
        desc: &Descriptor,
        data: Option<&DocData>,
        arena: &Arena,
        result: &mut SectionAnalysisResult,
    ) {
        let path = field.path_string();

        let found = match data {
            Some(d) => lookup(arena, d, &field.path.segments),
            None => Lookup::Missing,
        };

        // Normalise every lookup outcome to: kind, where it is, the literal
        // (if there is one), and an item count (if it is a collection).
        let (kind, position, literal, item_count): (Kind, Position, Option<ValueId>, Option<usize>) = match found {
            Lookup::Missing => {
                if desc.required {
                    self.add_error(
                        result,
                        Code::MissingRequired,
                        format!("Required @DATA field '{}' is missing", path),
                        format!("Add '{}' to @DATA, or declare it Schema.{}(false, ...)", path, capitalise(&desc.type_name)),
                        Some(field.position),
                    );
                }
                return;
            }
            Lookup::Table { position } => (Kind::Object, position, None, None),
            Lookup::Group { len, position } => (Kind::Array, position, None, Some(len)),
            Lookup::Value { value, declared, position } => {
                let literal_kind = kind_of_value(arena, arena.value(value));
                let declared_kind = declared.and_then(kind_of_declared);
                let effective = match (&literal_kind, declared_kind) {
                    (Kind::Null, _)                    => Kind::Null,
                    (Kind::Enum(_), Some(Kind::Enum(_))) => literal_kind.clone(),
                    (_, Some(d))                       => d,
                    (_, None)                          => literal_kind.clone(),
                };
                let count = match arena.value(value) {
                    ValueNode::Array { values, .. } | ValueNode::NestedArray { values, .. } => Some(values.len()),
                    _ => None,
                };
                (effective, position, Some(value), count)
            }
        };

        // A null value counts as absent.
        if matches!(kind, Kind::Null) {
            if desc.required {
                self.add_error(
                    result,
                    Code::MissingRequired,
                    format!("Required @DATA field '{}' is null", path),
                    "Give it a value, or declare it Schema.X(false, ...)".to_string(),
                    Some(position),
                );
            }
            return;
        }

        if !is_compatible(&desc.type_name, &kind) {
            self.add_error(
                result,
                Code::TypeMismatch,
                format!("@DATA field '{}' is {}, but @SCHEMA expects {}", path, kind.name(), desc.type_name),
                format!("Change the value to a {}, or change the schema for '{}'", desc.type_name, path),
                Some(position),
            );
            return;
        }

        // Constraint checks — only where the value is actually known.
        match desc.type_name.as_str() {
            "int" | "long" | "float" | "double" => {
                if let Some(n) = literal.and_then(|id| numeric_value(arena.value(id))) {
                    if let Some(min) = desc.min {
                        if n < min {
                            self.add_error(
                                result,
                                Code::OutOfRange,
                                format!("@DATA field '{}' = {} is below the minimum {}", path, n, min),
                                format!("Use a value >= {}", min),
                                Some(position),
                            );
                        }
                    }
                    if let Some(max) = desc.max {
                        if n > max {
                            self.add_error(
                                result,
                                Code::OutOfRange,
                                format!("@DATA field '{}' = {} is above the maximum {}", path, n, max),
                                format!("Use a value <= {}", max),
                                Some(position),
                            );
                        }
                    }
                }
            }
            "string" => {
                if let Some(ValueNode::String { value, .. }) = literal.map(|id| arena.value(id)) {
                    let len = arena.text(*value).chars().count() as i64;
                    if let Some(min) = desc.min_length {
                        if len < min {
                            self.add_error(
                                result,
                                Code::LengthViolation,
                                format!("@DATA field '{}' has length {}, below the minimum {}", path, len, min),
                                format!("Use at least {} character(s)", min),
                                Some(position),
                            );
                        }
                    }
                    if let Some(max) = desc.max_length {
                        if len > max {
                            self.add_error(
                                result,
                                Code::LengthViolation,
                                format!("@DATA field '{}' has length {}, above the maximum {}", path, len, max),
                                format!("Use at most {} character(s)", max),
                                Some(position),
                            );
                        }
                    }
                }
            }
            "array" => {
                if let Some(count) = item_count {
                    let count = count as i64;
                    if let Some(min) = desc.min_items {
                        if count < min {
                            self.add_error(
                                result,
                                Code::LengthViolation,
                                format!("@DATA field '{}' has {} item(s), below the minimum {}", path, count, min),
                                format!("Provide at least {} item(s)", min),
                                Some(position),
                            );
                        }
                    }
                    if let Some(max) = desc.max_items {
                        if count > max {
                            self.add_error(
                                result,
                                Code::LengthViolation,
                                format!("@DATA field '{}' has {} item(s), above the maximum {}", path, count, max),
                                format!("Provide at most {} item(s)", max),
                                Some(position),
                            );
                        }
                    }
                }
            }
            "enum" => {
                if let (Some(expected), Kind::Enum(Some(actual))) = (desc.enum_name.as_deref(), &kind) {
                    if !enum_names_match(actual, expected) {
                        self.add_error(
                            result,
                            Code::EnumMismatch,
                            format!("@DATA field '{}' is a value of enum '{}', but @SCHEMA expects enum '{}'", path, actual, expected),
                            format!("Use a value of enum '{}'", expected),
                            Some(position),
                        );
                    }
                }
            }
            _ => {}
        }
    }

    // ═════════════════════════════════════════════════════════════════════

    #[inline]
    fn should_halt(&self, result: &SectionAnalysisResult) -> bool {
        !result.errors.is_empty()
            && self.operational_settings.error_handling_strategy == ErrorHandlingStrategy::Halt
    }

    fn add_error(
        &mut self,
        result: &mut SectionAnalysisResult,
        code: Code,
        message: String,
        suggestion: String,
        position: Option<Position>,
    ) {
        let (line, col) = position.map(|p| (p.line as i32, p.column as i32)).unwrap_or((0, 0));

        self.error_manager.add_semantic_error(
            code.semantic_type(),
            message.clone(),
            line, col,
            Some("SCHEMA".to_string()),
            Some(suggestion.clone()),
        );

        result.errors.push(SemanticErrorInfo {
            error_id:     code.id().to_string(),
            error_type:   code.type_name().to_string(),
            message,
            section_name: "SCHEMA".to_string(),
            suggestion,
            position,
        });
    }

    fn add_warning(
        &mut self,
        result: &mut SectionAnalysisResult,
        warning_id: &str,
        message: &str,
        position: Option<Position>,
    ) {
        result.warnings.push(SemanticWarningInfo {
            warning_id:   warning_id.to_string(),
            message:      message.to_string(),
            section_name: "SCHEMA".to_string(),
            position,
        });
        if self.debug_config.is_enabled {
            self.error_manager.log_warning(message);
        }
    }
}

/// `"int"` -> `"Int"`, for suggestion text.
fn capitalise(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}
