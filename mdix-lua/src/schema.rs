// mdix-lua/src/schema.rs
//
// Schema validation for Lua — thin bindings over dixscript::Runtime::schema.
//
//   local schema = mdix.schema()
//       :require_string("app_name")
//       :require_int("port")
//       :require_long("created_at_ms")
//       :optional_bool("debug")
//
//   local report = db:validate_schema(schema)
//   if not report:is_valid() then
//       print(report:to_string())
//       for _, path in ipairs(report:failed_paths()) do
//           print("failed:", path)
//       end
//   end
//
// Custom validators (`require_with` / `optional_with` in the Rust core) are
// not exposed here on purpose — they take a `Fn(&DixData) -> Result<(), String>
// + Send + Sync + 'static` closure, and Lua functions aren't Send + Sync,
// so bridging that safely is its own separate piece of work. The named
// require_*/optional_* convenience methods below cover the overwhelming
// majority of real schema use; flag if you want custom validators wired up
// as a follow-up.

use mlua::{
    Lua, MetaMethod, Result as LuaResult,
    Table as LuaTable, UserData, UserDataMethods,
};
use dixscript::Runtime::{SchemaBuilder, ValidationReport};

use crate::error::closed_err;

// ── LuaMdixSchema ────────────────────────────────────────────────────────────

/// Wraps `SchemaBuilder`. The core builder's `require_*` / `optional_*` /
/// `with_description` methods consume `self` by value (fluent style), so
/// this stores `Option<SchemaBuilder>` and takes it out on every mutating
/// call, the same "take, mutate, put back" pattern used for the builder
/// fields elsewhere in this crate.
pub struct LuaMdixSchema {
    inner: Option<SchemaBuilder>,
}

impl LuaMdixSchema {
    pub fn new() -> Self {
        LuaMdixSchema { inner: Some(SchemaBuilder::new()) }
    }

    fn take(&mut self) -> LuaResult<SchemaBuilder> {
        self.inner.take().ok_or_else(closed_err)
    }

    /// Used by `Database:validate_schema` (in database.rs) to call
    /// `SchemaBuilder::validate(&self, data)` directly — deliberately NOT
    /// going through `DixData::validate_schema`, since that one takes the
    /// schema *by value*, which would mean moving it out of Lua userdata
    /// the caller might still want to reuse afterwards.
    pub(crate) fn as_builder(&self) -> LuaResult<&SchemaBuilder> {
        self.inner.as_ref().ok_or_else(closed_err)
    }
}

impl UserData for LuaMdixSchema {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {

        // ── required ─────────────────────────────────────────────────────

        methods.add_method_mut("require_string", |_, this, path: String| {
            let b = this.take()?;
            this.inner = Some(b.require_string(path));
            Ok(())
        });
        methods.add_method_mut("require_int", |_, this, path: String| {
            let b = this.take()?;
            this.inner = Some(b.require_int(path));
            Ok(())
        });
        /// Requires a 64-bit integer field. Also accepts Int values
        /// (an i32 widens into the i64 field with no precision loss).
        methods.add_method_mut("require_long", |_, this, path: String| {
            let b = this.take()?;
            this.inner = Some(b.require_long(path));
            Ok(())
        });
        methods.add_method_mut("require_float", |_, this, path: String| {
            let b = this.take()?;
            this.inner = Some(b.require_float(path));
            Ok(())
        });
        methods.add_method_mut("require_double", |_, this, path: String| {
            let b = this.take()?;
            this.inner = Some(b.require_double(path));
            Ok(())
        });
        methods.add_method_mut("require_bool", |_, this, path: String| {
            let b = this.take()?;
            this.inner = Some(b.require_bool(path));
            Ok(())
        });
        methods.add_method_mut("require_array", |_, this, path: String| {
            let b = this.take()?;
            this.inner = Some(b.require_array(path));
            Ok(())
        });
        methods.add_method_mut("require_object", |_, this, path: String| {
            let b = this.take()?;
            this.inner = Some(b.require_object(path));
            Ok(())
        });
        methods.add_method_mut("require_enum", |_, this, path: String| {
            let b = this.take()?;
            this.inner = Some(b.require_enum(path));
            Ok(())
        });

        // ── optional ─────────────────────────────────────────────────────

        methods.add_method_mut("optional_string", |_, this, path: String| {
            let b = this.take()?;
            this.inner = Some(b.optional_string(path));
            Ok(())
        });
        methods.add_method_mut("optional_int", |_, this, path: String| {
            let b = this.take()?;
            this.inner = Some(b.optional_int(path));
            Ok(())
        });
        methods.add_method_mut("optional_long", |_, this, path: String| {
            let b = this.take()?;
            this.inner = Some(b.optional_long(path));
            Ok(())
        });
        methods.add_method_mut("optional_float", |_, this, path: String| {
            let b = this.take()?;
            this.inner = Some(b.optional_float(path));
            Ok(())
        });
        methods.add_method_mut("optional_double", |_, this, path: String| {
            let b = this.take()?;
            this.inner = Some(b.optional_double(path));
            Ok(())
        });
        methods.add_method_mut("optional_bool", |_, this, path: String| {
            let b = this.take()?;
            this.inner = Some(b.optional_bool(path));
            Ok(())
        });
        methods.add_method_mut("optional_array", |_, this, path: String| {
            let b = this.take()?;
            this.inner = Some(b.optional_array(path));
            Ok(())
        });
        methods.add_method_mut("optional_object", |_, this, path: String| {
            let b = this.take()?;
            this.inner = Some(b.optional_object(path));
            Ok(())
        });

        // ── metadata ─────────────────────────────────────────────────────

        /// Annotates the most recently added field with a description.
        methods.add_method_mut("with_description", |_, this, description: String| {
            let b = this.take()?;
            this.inner = Some(b.with_description(description));
            Ok(())
        });

        methods.add_method("field_count", |_, this, ()| {
            Ok(this.as_builder()?.field_count() as i64)
        });

        methods.add_method("paths", |lua, this, ()| {
            let t = lua.create_table()?;
            for (i, p) in this.as_builder()?.paths().into_iter().enumerate() {
                t.set(i + 1, p)?;
            }
            Ok(t)
        });

        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            Ok(format!("MdixSchema(fields={})", this.as_builder().map(|b| b.field_count()).unwrap_or(0)))
        });
    }
}

// ── LuaMdixValidationReport ──────────────────────────────────────────────────

/// Wraps `ValidationReport`, returned by `Database:validate_schema`.
pub struct LuaMdixValidationReport {
    inner: ValidationReport,
}

impl LuaMdixValidationReport {
    pub(crate) fn new(report: ValidationReport) -> Self {
        LuaMdixValidationReport { inner: report }
    }
}

impl UserData for LuaMdixValidationReport {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {

        /// `true` when no validation errors were found.
        methods.add_method("is_valid", |_, this, ()| {
            Ok(this.inner.is_valid())
        });

        methods.add_method("error_count", |_, this, ()| {
            Ok(this.inner.error_count() as i64)
        });

        /// Dotted paths that failed validation, in order.
        methods.add_method("failed_paths", |lua, this, ()| {
            let t = lua.create_table()?;
            for (i, p) in this.inner.failed_paths().into_iter().enumerate() {
                t.set(i + 1, p)?;
            }
            Ok(t)
        });

        /// All errors as an array of tables:
        /// { path, expected, actual, kind } where kind is one of
        /// "Missing" | "WrongType" | "InvalidValue".
        methods.add_method("errors", |lua, this, ()| {
            let t = lua.create_table()?;
            for (i, e) in this.inner.errors.iter().enumerate() {
                let row: LuaTable = lua.create_table()?;
                row.set("path", e.path.clone())?;
                row.set("expected", e.expected.clone())?;
                row.set("actual", e.actual.clone())?;
                row.set("kind", e.kind.to_string())?;
                t.set(i + 1, row)?;
            }
            Ok(t)
        });

        /// Human-readable multi-line summary, identical to what
        /// `tostring(report)` already produces.
        methods.add_method("to_string", |_, this, ()| {
            Ok(this.inner.to_string())
        });

        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            Ok(this.inner.to_string())
        });
    }
}
