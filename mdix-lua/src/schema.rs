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
    AnyUserData, Lua, MetaMethod, Result as LuaResult,
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

        // require_*/optional_*/with_description all mutate the builder
        // and are meant to chain in Lua the same way schema.rs's own
        // header doc shows:
        //   mdix.schema():require_string("x"):require_int("y")
        // That can't work through add_method_mut — mlua only hands that
        // closure &mut Self, and returning Ok(()) (what this block used
        // to do) makes Lua's chain evaluate `(nil):require_int(...)`
        // once you're two calls deep, which raises. add_function_mut
        // instead takes the AnyUserData handle itself as the first
        // argument (mlua's own doc on it: "The first argument will be
        // an AnyUserData... if the method is called with Lua method
        // syntax"), borrows through it, and hands that same handle back
        // — genuinely chainable, not just documented as if it were.
        //
        // One macro instead of eighteen near-identical closures, since
        // every one of these differs only in which SchemaBuilder method
        // it forwards to.
        macro_rules! chainable_field_method {
            ($lua_name:literal, $core_method:ident) => {
                methods.add_function_mut($lua_name, |_, (this, path): (AnyUserData, String)| {
                    let mut b = this.borrow_mut::<LuaMdixSchema>()?;
                    let inner = b.take()?;
                    b.inner = Some(inner.$core_method(path));
                    drop(b);
                    Ok(this)
                });
            };
        }

        // ── required ─────────────────────────────────────────────────────

        chainable_field_method!("require_string", require_string);
        chainable_field_method!("require_int", require_int);
        /// Requires a 64-bit integer field. Also accepts Int values
        /// (an i32 widens into the i64 field with no precision loss).
        chainable_field_method!("require_long", require_long);
        chainable_field_method!("require_float", require_float);
        chainable_field_method!("require_double", require_double);
        chainable_field_method!("require_bool", require_bool);
        chainable_field_method!("require_array", require_array);
        chainable_field_method!("require_object", require_object);
        chainable_field_method!("require_enum", require_enum);

        // ── optional ─────────────────────────────────────────────────────

        chainable_field_method!("optional_string", optional_string);
        chainable_field_method!("optional_int", optional_int);
        chainable_field_method!("optional_long", optional_long);
        chainable_field_method!("optional_float", optional_float);
        chainable_field_method!("optional_double", optional_double);
        chainable_field_method!("optional_bool", optional_bool);
        chainable_field_method!("optional_array", optional_array);
        chainable_field_method!("optional_object", optional_object);

        // ── metadata ─────────────────────────────────────────────────────

        /// Annotates the most recently added field with a description.
        methods.add_function_mut("with_description", |_, (this, description): (AnyUserData, String)| {
            let mut b = this.borrow_mut::<LuaMdixSchema>()?;
            let inner = b.take()?;
            b.inner = Some(inner.with_description(description));
            drop(b);
            Ok(this)
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
