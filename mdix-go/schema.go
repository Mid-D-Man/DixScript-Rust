// schema.go — fluent schema validation for a loaded Database.
//
// Go counterpart to Python's MdixSchemaBuilder (mdix-python/src/schema.rs)
// and C#'s MdixSchemaBuilder (mdix-csharp/src/MidManStudio.Mdix.Core/MdixSchema.cs).
// Unlike Python (which binds the core's typed SchemaBuilder/ValidationReport
// directly via PyO3), this validates purely with Exists() / ValueTypeAt() —
// the same two calls any Go caller could make by hand — because mdix-ffi
// does not expose a schema-validation C ABI (confirmed against
// mdix-ffi/src/lib.rs; there is no mdix_schema_* extern "C" surface).
// C#'s MdixSchemaBuilder does the same thing for the same reason: it goes
// through the FFI too, it just happens to already be client-side.
//
// Per-field custom validators (C#'s RequireWith/OptionalWith closures) are
// intentionally not included — call Validate() first, then apply your own
// Go validation function to the loaded Database for anything more complex
// than "does this path exist with this type".
package dixscript

import "fmt"

// MdixValidationErrorKind classifies why a schema field failed validation.
type MdixValidationErrorKind int

const (
	// ValidationMissing: a required field's path does not exist.
	ValidationMissing MdixValidationErrorKind = iota
	// ValidationWrongType: the path exists but holds a different type
	// than the schema declared.
	ValidationWrongType
)

func (k MdixValidationErrorKind) String() string {
	switch k {
	case ValidationMissing:
		return "Missing"
	case ValidationWrongType:
		return "WrongType"
	default:
		return "Unknown"
	}
}

// MdixValidationError is a single field-validation failure.
type MdixValidationError struct {
	Path     string
	Expected ValueType
	Actual   ValueType // zero value (TypeNull) when Kind is ValidationMissing
	Kind     MdixValidationErrorKind
}

// Error implements the standard error interface, so a single
// MdixValidationError can be used anywhere a Go error is expected.
func (e MdixValidationError) Error() string {
	if e.Kind == ValidationMissing {
		return fmt.Sprintf("%q: missing required field (expected %s)", e.Path, e.Expected)
	}
	return fmt.Sprintf("%q: expected %s, got %s", e.Path, e.Expected, e.Actual)
}

// MdixValidationReport is the result of a full schema validation pass.
// Never an error itself — Validate() always returns one, valid or not, so
// the caller sees every failure at once instead of stopping at the first.
type MdixValidationReport struct {
	Errors []MdixValidationError
}

// IsValid reports whether every declared field passed.
func (r MdixValidationReport) IsValid() bool { return len(r.Errors) == 0 }

// ErrorCount returns the number of failed fields.
func (r MdixValidationReport) ErrorCount() int { return len(r.Errors) }

// FailedPaths returns just the paths that failed, in validation order.
func (r MdixValidationReport) FailedPaths() []string {
	out := make([]string, len(r.Errors))
	for i, e := range r.Errors {
		out[i] = e.Path
	}
	return out
}

func (r MdixValidationReport) String() string {
	if r.IsValid() {
		return "MdixValidationReport(valid)"
	}
	return fmt.Sprintf("MdixValidationReport(%d error(s))", len(r.Errors))
}

type schemaField struct {
	path     string
	expected ValueType
	required bool
}

// SchemaBuilder is a fluent, reusable schema definition. Not single-use —
// Validate() only reads from db, so the same SchemaBuilder can validate
// any number of databases.
//
//	report := dixscript.NewSchema().
//	    RequireString("app_name").
//	    RequireInt("port").
//	    OptionalBool("debug").
//	    Validate(db)
//
//	if !report.IsValid() {
//	    for _, e := range report.Errors {
//	        fmt.Println(e)
//	    }
//	}
type SchemaBuilder struct {
	fields []schemaField
}

// NewSchema starts an empty schema definition.
func NewSchema() *SchemaBuilder {
	return &SchemaBuilder{}
}

// Require declares path as required, expecting typ. Returns s for chaining.
func (s *SchemaBuilder) Require(path string, typ ValueType) *SchemaBuilder {
	s.fields = append(s.fields, schemaField{path: path, expected: typ, required: true})
	return s
}

// Optional declares path as optional — validated as typ only if present.
// Returns s for chaining.
func (s *SchemaBuilder) Optional(path string, typ ValueType) *SchemaBuilder {
	s.fields = append(s.fields, schemaField{path: path, expected: typ, required: false})
	return s
}

// ── Require* convenience wrappers ────────────────────────────────────────

func (s *SchemaBuilder) RequireString(path string) *SchemaBuilder { return s.Require(path, TypeString) }
func (s *SchemaBuilder) RequireInt(path string) *SchemaBuilder    { return s.Require(path, TypeInt) }
func (s *SchemaBuilder) RequireLong(path string) *SchemaBuilder   { return s.Require(path, TypeLong) }
func (s *SchemaBuilder) RequireFloat(path string) *SchemaBuilder  { return s.Require(path, TypeFloat) }
func (s *SchemaBuilder) RequireDouble(path string) *SchemaBuilder { return s.Require(path, TypeDouble) }
func (s *SchemaBuilder) RequireBool(path string) *SchemaBuilder   { return s.Require(path, TypeBool) }
func (s *SchemaBuilder) RequireDate(path string) *SchemaBuilder   { return s.Require(path, TypeDate) }
func (s *SchemaBuilder) RequireTimestamp(path string) *SchemaBuilder {
	return s.Require(path, TypeTimestamp)
}
func (s *SchemaBuilder) RequireHexColor(path string) *SchemaBuilder {
	return s.Require(path, TypeHexColor)
}
func (s *SchemaBuilder) RequireBlob(path string) *SchemaBuilder  { return s.Require(path, TypeBlob) }
func (s *SchemaBuilder) RequireRegex(path string) *SchemaBuilder { return s.Require(path, TypeRegex) }
func (s *SchemaBuilder) RequireArray(path string) *SchemaBuilder { return s.Require(path, TypeArray) }
func (s *SchemaBuilder) RequireObject(path string) *SchemaBuilder {
	return s.Require(path, TypeObject)
}
func (s *SchemaBuilder) RequireTuple(path string) *SchemaBuilder { return s.Require(path, TypeTuple) }
func (s *SchemaBuilder) RequireEnum(path string) *SchemaBuilder  { return s.Require(path, TypeEnum) }

// ── Optional* convenience wrappers ───────────────────────────────────────

func (s *SchemaBuilder) OptionalString(path string) *SchemaBuilder {
	return s.Optional(path, TypeString)
}
func (s *SchemaBuilder) OptionalInt(path string) *SchemaBuilder  { return s.Optional(path, TypeInt) }
func (s *SchemaBuilder) OptionalLong(path string) *SchemaBuilder { return s.Optional(path, TypeLong) }
func (s *SchemaBuilder) OptionalFloat(path string) *SchemaBuilder {
	return s.Optional(path, TypeFloat)
}
func (s *SchemaBuilder) OptionalDouble(path string) *SchemaBuilder {
	return s.Optional(path, TypeDouble)
}
func (s *SchemaBuilder) OptionalBool(path string) *SchemaBuilder { return s.Optional(path, TypeBool) }
func (s *SchemaBuilder) OptionalArray(path string) *SchemaBuilder {
	return s.Optional(path, TypeArray)
}
func (s *SchemaBuilder) OptionalObject(path string) *SchemaBuilder {
	return s.Optional(path, TypeObject)
}

// FieldCount returns the number of fields declared so far.
func (s *SchemaBuilder) FieldCount() int { return len(s.fields) }

// Paths returns every declared field path, in declaration order.
func (s *SchemaBuilder) Paths() []string {
	out := make([]string, len(s.fields))
	for i, f := range s.fields {
		out[i] = f.path
	}
	return out
}

// Validate checks every declared field against db and returns a full
// report. Does not stop at the first failure. Safe to call on a closed
// Database — every field simply reports as missing.
func (s *SchemaBuilder) Validate(db *Database) MdixValidationReport {
	var errs []MdixValidationError
	for _, f := range s.fields {
		if !db.Exists(f.path) {
			if f.required {
				errs = append(errs, MdixValidationError{
					Path: f.path, Expected: f.expected, Kind: ValidationMissing,
				})
			}
			continue
		}
		actual := db.ValueTypeAt(f.path)
		if !schemaTypeMatches(f.expected, actual) {
			errs = append(errs, MdixValidationError{
				Path: f.path, Expected: f.expected, Actual: actual, Kind: ValidationWrongType,
			})
		}
	}
	return MdixValidationReport{Errors: errs}
}

// schemaTypeMatches mirrors mdix-ffi's own getter behavior exactly rather
// than inventing looser rules of its own — a schema pass should never
// approve a field that the corresponding Get* call would then fail on.
// Checked directly against mdix-ffi/src/lib.rs:
//
//   - Float / Double are fully symmetric: mdix_get_float and
//     mdix_get_double both route through the same get::<f64>() call
//     internally (mdix_get_float just narrows the result to f32
//     afterward), so either type satisfies either expectation.
//   - Int / Long are NOT symmetric: mdix_get_long's doc comment states it
//     "also accepts Int values (widened without loss)", but mdix_get_int
//     uses a distinct get::<i32>() accessor with no such note — so
//     RequireLong accepts an actual Int, but RequireInt does not accept
//     an actual Long (GetInt would simply fail on it).
func schemaTypeMatches(expected, actual ValueType) bool {
	if expected == actual {
		return true
	}
	switch expected {
	case TypeLong:
		return actual == TypeInt
	case TypeFloat, TypeDouble:
		return actual == TypeFloat || actual == TypeDouble
	}
	return false
}
