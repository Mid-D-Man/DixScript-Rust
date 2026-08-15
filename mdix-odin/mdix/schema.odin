package mdix

// schema.odin — validating a loaded Database against a declared shape.
//
// mdix-ffi has no schema-validation C ABI (no mdix_schema_* extern "C"
// surface exists in mdix-ffi/src/lib.rs), so — same as the Go and C#
// bindings — this validates purely with exists() / get_type(), the same
// two calls any caller of this package could make by hand.
//
// Odin doesn't have method-call chaining in this toolchain (`x->f()` was
// tried and rejected by the compiler used to build this package — see
// the commit that added this file), so unlike Go/C#'s fluent
// `.RequireString(...).RequireInt(...)`, this is called as a sequence of
// plain procs against a `^Schema_Builder`, matching how builder_set_*
// is already used elsewhere in this package:
//
//   schema := schema_new()
//   defer schema_destroy(&schema)
//   schema_require_string(&schema, "app_name")
//   schema_require_int(&schema, "port")
//   schema_optional_bool(&schema, "debug")
//
//   report := schema_validate(schema, db)
//   defer validation_report_destroy(&report)
//   if !validation_report_is_valid(report) {
//       for e in report.errors {
//           fmt.println(validation_error_to_string(e))
//       }
//   }

import "core:fmt"
import ffi "../mdix_ffi"

Validation_Error_Kind :: enum {
	Missing,    // a required field's path does not exist
	Wrong_Type, // the path exists but holds a different type than declared
}

Validation_Error :: struct {
	path:     string,
	expected: ffi.Mdix_Type,
	actual:   ffi.Mdix_Type, // .Unknown when kind == .Missing
	kind:     Validation_Error_Kind,
}

validation_error_to_string :: proc(e: Validation_Error, allocator := context.allocator) -> string {
	if e.kind == .Missing {
		return fmt.aprintf("%q: missing required field (expected %v)", e.path, e.expected, allocator = allocator)
	}
	return fmt.aprintf("%q: expected %v, got %v", e.path, e.expected, e.actual, allocator = allocator)
}

// Validation_Report is the result of a full schema_validate pass — never
// an error by itself, so every failure is visible at once instead of
// stopping at the first.
Validation_Report :: struct {
	errors: [dynamic]Validation_Error,
}

validation_report_is_valid :: proc(r: Validation_Report) -> bool {
	return len(r.errors) == 0
}

validation_report_destroy :: proc(r: ^Validation_Report) {
	delete(r.errors)
}

@(private = "file")
Schema_Field :: struct {
	path:     string,
	expected: ffi.Mdix_Type,
	required: bool,
}

// Schema_Builder is a reusable schema definition — schema_validate only
// reads from the Database you pass it, so the same Schema_Builder can
// validate any number of databases.
Schema_Builder :: struct {
	fields: [dynamic]Schema_Field,
}

schema_new :: proc(allocator := context.allocator) -> Schema_Builder {
	return Schema_Builder{fields = make([dynamic]Schema_Field, 0, allocator)}
}

schema_destroy :: proc(s: ^Schema_Builder) {
	delete(s.fields)
}

schema_field_count :: proc(s: Schema_Builder) -> int {
	return len(s.fields)
}

schema_require :: proc(s: ^Schema_Builder, path: string, expected: ffi.Mdix_Type) {
	append(&s.fields, Schema_Field{path = path, expected = expected, required = true})
}

schema_optional :: proc(s: ^Schema_Builder, path: string, expected: ffi.Mdix_Type) {
	append(&s.fields, Schema_Field{path = path, expected = expected, required = false})
}

// ── require_* / optional_* convenience wrappers ────────────────────────

schema_require_string :: proc(s: ^Schema_Builder, path: string) { schema_require(s, path, .String) }
schema_require_int    :: proc(s: ^Schema_Builder, path: string) { schema_require(s, path, .Int) }
schema_require_long   :: proc(s: ^Schema_Builder, path: string) { schema_require(s, path, .Long) }
schema_require_float  :: proc(s: ^Schema_Builder, path: string) { schema_require(s, path, .Float) }
schema_require_double :: proc(s: ^Schema_Builder, path: string) { schema_require(s, path, .Double) }
schema_require_bool   :: proc(s: ^Schema_Builder, path: string) { schema_require(s, path, .Bool) }
schema_require_array  :: proc(s: ^Schema_Builder, path: string) { schema_require(s, path, .Array) }
schema_require_object :: proc(s: ^Schema_Builder, path: string) { schema_require(s, path, .Object) }
schema_require_enum   :: proc(s: ^Schema_Builder, path: string) { schema_require(s, path, .Enum) }

schema_optional_string :: proc(s: ^Schema_Builder, path: string) { schema_optional(s, path, .String) }
schema_optional_int    :: proc(s: ^Schema_Builder, path: string) { schema_optional(s, path, .Int) }
schema_optional_long   :: proc(s: ^Schema_Builder, path: string) { schema_optional(s, path, .Long) }
schema_optional_float  :: proc(s: ^Schema_Builder, path: string) { schema_optional(s, path, .Float) }
schema_optional_double :: proc(s: ^Schema_Builder, path: string) { schema_optional(s, path, .Double) }
schema_optional_bool   :: proc(s: ^Schema_Builder, path: string) { schema_optional(s, path, .Bool) }

// schema_validate checks every declared field against db and returns a
// full report — it does not stop at the first failure. Safe to call on
// a db with handle == nil; every field simply reports as missing.
schema_validate :: proc(s: Schema_Builder, db: Database, allocator := context.allocator) -> Validation_Report {
	errors := make([dynamic]Validation_Error, 0, allocator)
	for f in s.fields {
		if !exists(db, f.path) {
			if f.required {
				append(&errors, Validation_Error{path = f.path, expected = f.expected, kind = .Missing})
			}
			continue
		}
		actual := get_type(db, f.path)
		if !schema_type_matches(f.expected, actual) {
			append(&errors, Validation_Error{
				path = f.path, expected = f.expected, actual = actual, kind = .Wrong_Type,
			})
		}
	}
	return Validation_Report{errors = errors}
}

// schema_type_matches mirrors mdix-ffi's own getter behavior exactly
// rather than inventing looser rules — a schema pass should never
// approve a field the corresponding get_* proc would then fail to read.
// Checked directly against mdix-ffi/src/lib.rs, same reasoning as the Go
// binding's schemaTypeMatches (mdix-go/schema.go):
//
//   - Float / Double are fully symmetric: mdix_get_float and
//     mdix_get_double both route through the same internal get::<f64>()
//     call (mdix_get_float just narrows the result to f32 afterward), so
//     either type satisfies either expectation.
//   - Int / Long are NOT symmetric: mdix_get_long's doc comment states it
//     "also accepts Int values (widened without loss)", but mdix_get_int
//     uses a distinct get::<i32>() accessor with no such note — so
//     .Long accepts an actual .Int, but .Int does not accept an actual
//     .Long (get_int would simply fail on it).
@(private = "file")
schema_type_matches :: proc(expected, actual: ffi.Mdix_Type) -> bool {
	if expected == actual {
		return true
	}
	#partial switch expected {
	case .Long:
		return actual == .Int
	case .Float, .Double:
		return actual == .Float || actual == .Double
	case:
		return false
	}
}
