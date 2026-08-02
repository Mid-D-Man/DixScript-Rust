//! Shared local-variable type tracking for a QuickFunc body.
//!
//! `inlay_hints.rs`'s `collect_qf_var_hints` walks a function body in
//! statement order, building up a `running: HashMap<String,
//! Option<DataType>>` map as it goes — each `VariableDeclaration` gets
//! resolved (via `TypeInferenceVisitor`, seeded with everything resolved
//! *so far*) and inserted before moving on, so a later statement like
//! `b = a.first()` can see `a`'s already-resolved type. That's why inlay
//! hints correctly infer chained locals (including through tuple/array
//! element access) while hover and completions previously couldn't: both
//! only ever built a map from the enclosing function's *parameters*
//! (hover) or nothing at all (completions, before the previous fix) —
//! never from variables declared earlier in the same body. A parameter-
//! only or empty map resolves `x = someParam.first()` fine, but not
//! `x = someLocal.first()` where `someLocal` was itself declared two
//! lines above.
//!
//! This module pulls the type-tracking *core* of that walk out on its
//! own, with the hint-emission side effects (position/column math,
//! pushing `InlayHint`s, param-hint recursion, ...) stripped out, so
//! `hover.rs` and `completions.rs` can build the same kind of map without
//! duplicating (and inevitably drifting from) `inlay_hints.rs`'s version.
//! `inlay_hints.rs` itself is untouched — it already works, and re-basing
//! it onto this shared helper is a separate, lower-value change with more
//! risk than benefit for what's needed right now.
//!
//! Branch scoping matches `collect_qf_var_hints` exactly: `if`/`switch`
//! branches are walked with a *clone* of the running map, and whatever
//! they resolve does not propagate back out to the caller — a variable
//! declared inside one branch isn't visible after the branch ends, and
//! sibling branches don't see each other's locals.

use std::collections::HashMap;

use dixscript::Compiler::AST::{DataType, QuickFuncStatement, QuickFunction, TypeInferenceVisitor};
use dixscript::Compiler::Utilities::SymbolTable;

/// Full map of parameter + local-variable types visible anywhere in
/// `func`'s body — the main entry point most callers want. Parameters
/// seed the map (their declared type, or `None` if unannotated); each
/// local declaration is then resolved in statement order and inserted,
/// so later declarations can reference earlier ones.
pub fn local_variable_types_for_function(
    func:         &QuickFunction,
    symbol_table: Option<&SymbolTable>,
) -> HashMap<String, Option<DataType>> {
    let seed: HashMap<String, Option<DataType>> = func.parameters
        .iter()
        .map(|p| (p.name.clone(), p.data_type))
        .collect();
    build_local_variable_types(&func.body, seed, symbol_table)
}

/// Lower-level walk, for callers that already have their own seed map
/// (e.g. a lambda body inheriting its enclosing scope) or want to run it
/// against a sub-slice of statements directly.
pub fn build_local_variable_types(
    stmts:        &[QuickFuncStatement],
    mut running:  HashMap<String, Option<DataType>>,
    symbol_table: Option<&SymbolTable>,
) -> HashMap<String, Option<DataType>> {
    for stmt in stmts {
        match stmt {
            QuickFuncStatement::VariableDeclaration { variable_name, data_type, value, .. } => {
                let resolved = match data_type {
                    Some(dt) => Some(*dt),
                    None => symbol_table.and_then(|st| {
                        TypeInferenceVisitor::new(st, Some(running.clone()))
                            .infer_type_from_expression(value)
                    }),
                };
                running.insert(variable_name.clone(), resolved);
            }
            QuickFuncStatement::If { then_branch, else_branch, .. } => {
                let _ = build_local_variable_types(then_branch, running.clone(), symbol_table);
                if let Some(eb) = else_branch {
                    let _ = build_local_variable_types(eb, running.clone(), symbol_table);
                }
            }
            QuickFuncStatement::Switch { cases, default_case, .. } => {
                for case in cases {
                    let _ = build_local_variable_types(&case.statements, running.clone(), symbol_table);
                }
                if let Some(dc) = default_case {
                    let _ = build_local_variable_types(&dc.statements, running.clone(), symbol_table);
                }
            }
            _ => {}
        }
    }
    running
}
