// benches/ast_enhancement_benchmark.rs
//! AST Enhancement Benchmark — DixScript v1.0.0
//!
//! Benchmarks the enhancement phase, focused on QualifiedIdentifier resolution
//! which is the exclusive hot path of QuickFunctions enhancement.
//! (Data section does not use QualifiedIdentifiers — QF is the only consumer.)
//!
//! ## Groups
//!
//! 1. `resolver_microbench`     — QualifiedIdentifierResolver::resolve_expression and
//!    resolve_statement for every resolution type + no-hit fallback + map-size sensitivity.
//!    Isolates pure HashMap-lookup + match + output-expression construction cost.
//!
//! 2. `enhancer_qi_density`     — QuickFunctionsAstEnhancer, 5 functions, varying QI
//!    count per function: 0 (baseline, parameter-defaults only), 5, 15, 30.
//!    Reveals the marginal cost of each resolved QI.
//!
//! 3. `enhancer_function_count` — Fixed 10 QIs per function, varying function count:
//!    1 / 5 / 10 / 25 / 50. Verifies linear scaling.
//!
//! 4. `object_access`           — ObjectPropertyAccess and instance method call resolution
//!    in isolation: single-level, nested, method calls, deep chains, and density scaling.
//!    Derives per-access cost and compares property vs method-call overhead.
//!
//! 5. `full_enhancement_pipeline` — GeneralAstEnhancer end-to-end on pre-parsed and
//!    pre-analyzed ASTs. Paired with parse+analyze baselines so enhancement overhead
//!    can be derived: enhance_cost = full_pipeline - semantics_bench/full_pipeline.
//!
//! ## Known hot-path allocation
//! QualifiedIdentifierResolver::transform_qualified_identifier always calls
//! `parts.join(".")` before the HashMap lookup, allocating a String unconditionally.
//! Results include this overhead until the resolver gates that call behind
//! debug_config.is_enabled per the Finalization.json policy.
//!
//! Add to Cargo.toml:
//!   [[bench]]
//!   name    = "ast_enhancement_benchmark"
//!   harness = false

use criterion::{
    black_box, criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion, Throughput,
};
use dixscript::Compiler::AST::{
    DataType, DeclarationType, DixScript, Expression, Position, QuickFunction, QuickFuncParam,
    QuickFuncStatement, QuickFuncsSection, Value,
};
use dixscript::Compiler::Core::{
    Config::{ConfigSectionHandler, OperationalSettings},
    GeneralAstEnhancer, GeneralParser, GeneralSemanticAnalyzer, SectionAnalysisResult,
    SemanticAnalysisResult,
    SectionEnhancers::{
        QualifiedIdentifierKey, QualifiedIdentifierResolution, QualifiedIdentifierResolver,
        QualifiedIdentifierType, QuickFunctionsAstEnhancer,
    },
    Tokenizer::Tokenizer,
};
use std::collections::HashMap;
use std::time::Duration;

// =============================================================================
// Constants
// =============================================================================

/// QI count / label pairs for density benchmarks (fixed 5 functions).
const QI_DENSITY_CASES: &[(usize, &str)] = &[
    (0,  "0qi_baseline"),
    (5,  "5qi_per_func"),
    (15, "15qi_per_func"),
    (30, "30qi_per_func"),
];

/// Function count / label pairs for scaling benchmarks (fixed 10 QIs/func).
const FUNC_COUNT_CASES: &[(usize, &str)] = &[
    (1,  "1func"),
    (5,  "5funcs"),
    (10, "10funcs"),
    (25, "25funcs"),
    (50, "50funcs"),
];

/// Object property access depth / label pairs.
const OBJECT_DEPTH_CASES: &[(usize, &str)] = &[
    (1, "depth_1_single"),
    (2, "depth_2_nested"),
    (3, "depth_3_deep"),
    (4, "depth_4_very_deep"),
];

/// Object count / label pairs for the object density bench (fixed 3 properties each).
const OBJECT_COUNT_CASES: &[(usize, &str)] = &[
    (1,  "1obj"),
    (5,  "5objs"),
    (10, "10objs"),
    (20, "20objs"),
];

const DENSITY_FIXED_FUNC_COUNT: usize = 5;
const SCALING_FIXED_QI_PER_FUNC: usize = 10;
const OBJECT_FIXED_PROPS_PER_OBJ: usize = 3;

// =============================================================================
// QI pattern table — 8 representative resolution scenarios
// =============================================================================

/// Produce one (Expression, resolved_type, parts, is_call) tuple for the given
/// `index % 8` pattern at `pos`.  Covers every QualifiedIdentifierType variant
/// used in real QuickFunctions code.
fn make_qi_at(
    index: usize,
    pos: Position,
) -> (Expression, QualifiedIdentifierType, Vec<String>, bool) {
    let build = |parts: Vec<String>, is_call: bool| -> Expression {
        Expression::QualifiedIdentifier {
            parts: parts.clone(),
            arguments: if is_call { Some(vec![]) } else { None },
            position: pos,
        }
    };

    match index % 8 {
        0 => {
            let p = vec!["Status".to_string(), "ACTIVE".to_string()];
            (build(p.clone(), false), QualifiedIdentifierType::LocalEnumAccess, p, false)
        }
        1 => {
            let p = vec!["LogLevel".to_string(), "WARN".to_string()];
            (build(p.clone(), false), QualifiedIdentifierType::LocalEnumAccess, p, false)
        }
        2 => {
            let p = vec!["utils".to_string(), "Status".to_string(), "PENDING".to_string()];
            (build(p.clone(), false), QualifiedIdentifierType::ImportedEnumAccess, p, false)
        }
        3 => {
            let p = vec!["helpers".to_string(), "calculate".to_string()];
            (build(p.clone(), true), QualifiedIdentifierType::ImportedFunctionCall, p, true)
        }
        4 => {
            let p = vec!["Math".to_string(), "sqrt".to_string()];
            (build(p.clone(), true), QualifiedIdentifierType::StaticObjectAccess, p, true)
        }
        5 => {
            let p = vec!["DateTime".to_string(), "now".to_string()];
            (build(p.clone(), true), QualifiedIdentifierType::StaticObjectAccess, p, true)
        }
        6 => {
            let p = vec!["user".to_string(), "name".to_string()];
            (build(p.clone(), false), QualifiedIdentifierType::ObjectPropertyAccess, p, false)
        }
        _ => {
            let p = vec!["text".to_string(), "toUpper".to_string()];
            (build(p.clone(), true), QualifiedIdentifierType::ObjectPropertyAccess, p, true)
        }
    }
}

// =============================================================================
// Object access helpers
// =============================================================================

/// Build an ObjectPropertyAccess expression with the given parts at `pos`.
fn make_object_property(parts: Vec<String>, pos: Position) -> Expression {
    Expression::QualifiedIdentifier {
        parts,
        arguments: None,
        position: pos,
    }
}

/// Build an ObjectPropertyAccess method call expression with the given parts at `pos`.
fn make_object_method(parts: Vec<String>, pos: Position) -> Expression {
    Expression::QualifiedIdentifier {
        parts,
        arguments: Some(vec![]),
        position: pos,
    }
}

/// Build a resolver and matching sample expression for a single ObjectPropertyAccess
/// chain of the given depth. Parts are: obj0, field1, field2, ... up to `depth`.
/// Returns (resolver, expression, key).
fn build_object_property_resolver(
    depth: usize,
    pos: Position,
) -> (QualifiedIdentifierResolver, Expression) {
    assert!(depth >= 1);
    let mut parts: Vec<String> = vec!["obj0".to_string()];
    for i in 1..depth {
        parts.push(format!("field{}", i));
    }

    let expr = make_object_property(parts.clone(), pos);
    let key = QualifiedIdentifierKey { position: pos, parts: parts.clone(), is_call: false };
    let resolution = QualifiedIdentifierResolution {
        resolved_type: QualifiedIdentifierType::ObjectPropertyAccess,
        context: None,
        parts: parts.clone(),
        is_call: false,
        position: pos,
    };

    let mut map = HashMap::with_capacity(1);
    map.insert(key, resolution);
    (QualifiedIdentifierResolver::new(map), expr)
}

/// Build a resolver and matching sample expression for an ObjectPropertyAccess
/// method call (obj.method()).
fn build_object_method_resolver(pos: Position) -> (QualifiedIdentifierResolver, Expression) {
    let parts = vec!["obj0".to_string(), "compute".to_string()];
    let expr = make_object_method(parts.clone(), pos);
    let key = QualifiedIdentifierKey { position: pos, parts: parts.clone(), is_call: true };
    let resolution = QualifiedIdentifierResolution {
        resolved_type: QualifiedIdentifierType::ObjectPropertyAccess,
        context: None,
        parts: parts.clone(),
        is_call: true,
        position: pos,
    };

    let mut map = HashMap::with_capacity(1);
    map.insert(key, resolution);
    (QualifiedIdentifierResolver::new(map), expr)
}

/// Build a function whose body contains `obj_count * props_per_obj` ObjectPropertyAccess
/// expressions — `props_per_obj` property accesses on each of `obj_count` distinct objects.
/// Returns the section and matching SectionAnalysisResult.
fn build_object_access_section(
    obj_count: usize,
    props_per_obj: usize,
    pos_base: usize,
) -> (QuickFuncsSection, SectionAnalysisResult) {
    let total_qi = obj_count * props_per_obj;
    let mut resolutions: HashMap<QualifiedIdentifierKey, QualifiedIdentifierResolution> =
        HashMap::with_capacity(total_qi);
    let mut body: Vec<QuickFuncStatement> = Vec::with_capacity(total_qi.max(1));

    let mut stmt_idx = 0usize;
    for obj_idx in 0..obj_count {
        for prop_idx in 0..props_per_obj {
            let pos = Position::new(pos_base + stmt_idx + 1, stmt_idx + 1);
            let parts = vec![
                format!("obj{}", obj_idx),
                format!("field{}", prop_idx),
            ];
            let is_last = stmt_idx + 1 == total_qi;
            let is_call = false;

            let expr = make_object_property(parts.clone(), pos);
            resolutions.insert(
                QualifiedIdentifierKey { position: pos, parts: parts.clone(), is_call },
                QualifiedIdentifierResolution {
                    resolved_type: QualifiedIdentifierType::ObjectPropertyAccess,
                    context: None,
                    parts,
                    is_call,
                    position: pos,
                },
            );

            if is_last {
                body.push(QuickFuncStatement::Return {
                    value: expr,
                    position: Position::UNKNOWN,
                });
            } else {
                body.push(QuickFuncStatement::VariableDeclaration {
                    declaration_type: DeclarationType::Let,
                    is_mutable: false,
                    variable_name: format!("v{}", stmt_idx),
                    data_type: None,
                    value: expr,
                    position: Position::UNKNOWN,
                });
            }
            stmt_idx += 1;
        }
    }

    if body.is_empty() {
        body.push(QuickFuncStatement::Return {
            value: Expression::Value {
                value: Value::Integer { value: 0, position: Position::UNKNOWN },
                position: Position::UNKNOWN,
            },
            position: Position::UNKNOWN,
        });
    }

    let func = QuickFunction {
        name: "obj_access_func".to_string(),
        return_type: Some(DataType::Any),
        scope_list: Some(vec!["global".to_string()]),
        parameters: vec![QuickFuncParam {
            name: "x".to_string(),
            data_type: Some(DataType::Int),
            default_value: None,
            position: Position::UNKNOWN,
        }],
        body,
        position: Position::UNKNOWN,
    };

    let section = QuickFuncsSection::new(vec![func], Position::UNKNOWN);
    let mut analysis = SectionAnalysisResult::new("QUICKFUNCS");
    analysis.is_success = true;
    analysis.qualified_id_resolutions = resolutions;

    (section, analysis)
}

/// Build a function whose body mixes property access and method calls on `obj_count`
/// objects — one property access and one method call per object.
fn build_mixed_object_access_section(
    obj_count: usize,
    pos_base: usize,
) -> (QuickFuncsSection, SectionAnalysisResult) {
    let total_qi = obj_count * 2;
    let mut resolutions: HashMap<QualifiedIdentifierKey, QualifiedIdentifierResolution> =
        HashMap::with_capacity(total_qi);
    let mut body: Vec<QuickFuncStatement> = Vec::with_capacity(total_qi.max(1));

    for obj_idx in 0..obj_count {
        // Property access
        let prop_pos = Position::new(pos_base + obj_idx * 2 + 1, obj_idx * 2 + 1);
        let prop_parts = vec![format!("obj{}", obj_idx), "name".to_string()];
        let prop_expr = make_object_property(prop_parts.clone(), prop_pos);
        resolutions.insert(
            QualifiedIdentifierKey { position: prop_pos, parts: prop_parts.clone(), is_call: false },
            QualifiedIdentifierResolution {
                resolved_type: QualifiedIdentifierType::ObjectPropertyAccess,
                context: None,
                parts: prop_parts,
                is_call: false,
                position: prop_pos,
            },
        );
        body.push(QuickFuncStatement::VariableDeclaration {
            declaration_type: DeclarationType::Let,
            is_mutable: false,
            variable_name: format!("prop{}", obj_idx),
            data_type: None,
            value: prop_expr,
            position: Position::UNKNOWN,
        });

        // Method call
        let call_pos = Position::new(pos_base + obj_idx * 2 + 2, obj_idx * 2 + 2);
        let call_parts = vec![format!("obj{}", obj_idx), "toString".to_string()];
        let call_expr = make_object_method(call_parts.clone(), call_pos);
        resolutions.insert(
            QualifiedIdentifierKey { position: call_pos, parts: call_parts.clone(), is_call: true },
            QualifiedIdentifierResolution {
                resolved_type: QualifiedIdentifierType::ObjectPropertyAccess,
                context: None,
                parts: call_parts,
                is_call: true,
                position: call_pos,
            },
        );

        if obj_idx + 1 == obj_count {
            body.push(QuickFuncStatement::Return {
                value: call_expr,
                position: Position::UNKNOWN,
            });
        } else {
            body.push(QuickFuncStatement::VariableDeclaration {
                declaration_type: DeclarationType::Let,
                is_mutable: false,
                variable_name: format!("call{}", obj_idx),
                data_type: None,
                value: call_expr,
                position: Position::UNKNOWN,
            });
        }
    }

    if body.is_empty() {
        body.push(QuickFuncStatement::Return {
            value: Expression::Value {
                value: Value::Integer { value: 0, position: Position::UNKNOWN },
                position: Position::UNKNOWN,
            },
            position: Position::UNKNOWN,
        });
    }

    let func = QuickFunction {
        name: "mixed_obj_access_func".to_string(),
        return_type: Some(DataType::Any),
        scope_list: Some(vec!["global".to_string()]),
        parameters: vec![],
        body,
        position: Position::UNKNOWN,
    };

    let section = QuickFuncsSection::new(vec![func], Position::UNKNOWN);
    let mut analysis = SectionAnalysisResult::new("QUICKFUNCS");
    analysis.is_success = true;
    analysis.qualified_id_resolutions = resolutions;

    (section, analysis)
}

// =============================================================================
// Construction helpers
// =============================================================================

/// Build a resolver loaded with `entries_per_type` entries for each of the 8 QI
/// patterns and return the 8 sample expressions (one per type, entry-0 position).
fn build_resolver(entries_per_type: usize)
    -> (QualifiedIdentifierResolver, Vec<Expression>)
{
    let total = entries_per_type * 8;
    let mut map: HashMap<QualifiedIdentifierKey, QualifiedIdentifierResolution> =
        HashMap::with_capacity(total);
    let mut sample_exprs: Vec<Expression> = Vec::with_capacity(8);

    for type_idx in 0..8usize {
        for entry in 0..entries_per_type {
            let pos = Position::new(type_idx * 100_000 + entry + 1, 1);
            let (expr, resolved_type, parts, is_call) = make_qi_at(type_idx, pos);

            if entry == 0 {
                sample_exprs.push(expr);
            }

            map.insert(
                QualifiedIdentifierKey { position: pos, parts: parts.clone(), is_call },
                QualifiedIdentifierResolution {
                    resolved_type,
                    context: None,
                    parts,
                    is_call,
                    position: pos,
                },
            );
        }
    }

    (QualifiedIdentifierResolver::new(map), sample_exprs)
}

/// Build one QuickFunction whose body contains `qi_count` QI expressions.
fn build_function(
    name: &str,
    qi_count: usize,
    pos_base: usize,
) -> (QuickFunction, HashMap<QualifiedIdentifierKey, QualifiedIdentifierResolution>) {
    let mut resolutions: HashMap<QualifiedIdentifierKey, QualifiedIdentifierResolution> =
        HashMap::with_capacity(qi_count);
    let mut body: Vec<QuickFuncStatement> = Vec::with_capacity(qi_count.max(1));

    for qi_idx in 0..qi_count {
        let pos = Position::new(pos_base + qi_idx + 1, qi_idx + 1);
        let (expr, resolved_type, parts, is_call) = make_qi_at(qi_idx, pos);

        resolutions.insert(
            QualifiedIdentifierKey { position: pos, parts: parts.clone(), is_call },
            QualifiedIdentifierResolution {
                resolved_type,
                context: None,
                parts,
                is_call,
                position: pos,
            },
        );

        if qi_idx + 1 < qi_count {
            body.push(QuickFuncStatement::VariableDeclaration {
                declaration_type: DeclarationType::Let,
                is_mutable:       false,
                variable_name:    format!("v{}", qi_idx),
                data_type:        None,
                value:            expr,
                position:         Position::UNKNOWN,
            });
        } else {
            body.push(QuickFuncStatement::Return {
                value:    expr,
                position: Position::UNKNOWN,
            });
        }
    }

    if body.is_empty() {
        body.push(QuickFuncStatement::Return {
            value: Expression::Value {
                value:    Value::Integer { value: 42, position: Position::UNKNOWN },
                position: Position::UNKNOWN,
            },
            position: Position::UNKNOWN,
        });
    }

    let func = QuickFunction {
        name:         name.to_string(),
        return_type:  Some(DataType::Any),
        scope_list:   Some(vec!["global".to_string()]),
        parameters:   vec![QuickFuncParam {
            name:          "x".to_string(),
            data_type:     Some(DataType::Int),
            default_value: None,
            position:      Position::UNKNOWN,
        }],
        body,
        position: Position::UNKNOWN,
    };

    (func, resolutions)
}

/// Build a (QuickFuncsSection, SectionAnalysisResult) pair.
fn build_section(
    func_count: usize,
    qi_per_func: usize,
    pos_base: usize,
) -> (QuickFuncsSection, SectionAnalysisResult) {
    let mut resolutions: HashMap<QualifiedIdentifierKey, QualifiedIdentifierResolution> =
        HashMap::with_capacity(func_count * qi_per_func);
    let mut functions: Vec<QuickFunction> = Vec::with_capacity(func_count);

    for fi in 0..func_count {
        let func_pos_base = pos_base + fi * (qi_per_func + 10);
        let (func, func_resolutions) =
            build_function(&format!("func_{}", fi), qi_per_func, func_pos_base);
        functions.push(func);
        resolutions.extend(func_resolutions);
    }

    let section = QuickFuncsSection::new(functions, Position::UNKNOWN);

    let mut analysis_result = SectionAnalysisResult::new("QUICKFUNCS");
    analysis_result.is_success = true;
    analysis_result.qualified_id_resolutions = resolutions;

    (section, analysis_result)
}

// =============================================================================
// Full-pipeline helpers
// =============================================================================

fn parse_to_ast(source: &str) -> (DixScript, OperationalSettings) {
    let mut handler = ConfigSectionHandler::new(None);
    let cfg = handler.process_config_section(source);
    let settings = cfg.operational_settings.clone();
    let toks = Tokenizer::new(&cfg.cleaned_input_string, &settings).tokenize();
    let parser =
        GeneralParser::new(toks.tokens, &cfg.config_section, &settings).expect("parser init");
    let ast = parser.parse().expect("parse failed in bench setup");
    (ast, settings)
}

fn parse_and_analyze(
    source: &str,
) -> (DixScript, OperationalSettings, SemanticAnalysisResult) {
    let (ast, settings) = parse_to_ast(source);
    let semantic_result = GeneralSemanticAnalyzer::new(&ast, &settings).analyze();
    (ast, settings, semantic_result)
}

// =============================================================================
// Static DixScript source inputs
// =============================================================================

const SMALL_SOURCE: &str = r#"@CONFIG(
    version        -> "1.0.0",
    encoding       -> "utf-8",
    features       -> "advanced",
    error_handling -> "halt"
)
@ENUMS(
    Status   { ACTIVE = 1, INACTIVE = 2, PENDING = 3 }
    LogLevel { DEBUG = 0, INFO = 1, WARN = 2, ERROR = 3 }
)
@QUICKFUNCS(
    ~formatStatus<string> => global(s<enum>) {
        return s == Status.ACTIVE ? "active" :
               s == Status.PENDING ? "pending" : "inactive"
    }
    ~buildLabel<string> => global(name<string>, level<enum>) {
        let prefix = level == LogLevel.ERROR ? "ERR" : "OK"
        return $"{prefix}:{name}"
    }
    ~clamp<int> => global(val<int>, lo<int>, hi<int>) {
        return val < lo ? lo : val > hi ? hi : val
    }
)
@DATA( x = 1 )"#;

const MEDIUM_SOURCE: &str = r#"@CONFIG(
    version        -> "1.0.0",
    encoding       -> "utf-8",
    features       -> "advanced",
    error_handling -> "halt"
)
@ENUMS(
    Status      { ACTIVE = 1, INACTIVE = 2, PENDING = 3, DELETED = 4 }
    LogLevel    { DEBUG = 0, INFO = 1, WARN = 2, ERROR = 3, FATAL = 4 }
    HttpMethod  { GET = 1, POST = 2, PUT = 3, DELETE = 4 }
    Environment { DEV = 1, STAGING = 2, PROD = 3 }
)
@QUICKFUNCS(
    ~createServer<object> => global(host<string>, port<int>, ssl<bool>) {
        return {
            host = host,
            port = port,
            ssl  = ssl,
            url  = $"https://{host}:{port}"
        }
    }
    ~poolSize<int> => global(env<int>, base<int>) {
        let multiplier = env == 3 ? 5 : env == 2 ? 2 : 1
        return base * multiplier
    }
    ~buildDbConfig<object> => global(host<string>, port<int>, name<string>, env<int>) {
        let pool = poolSize(env, 10)
        return {
            host = host, port = port, database = name,
            pool = pool, ssl = env == 3
        }
    }
    ~validatePort<bool> => global(port<int>) {
        return port > 1024 && port < 65536
    }
    ~formatLabel<string> => global(name<string>, version<string>) {
        return $"{name}-v{version}"
    }
    ~calcXp<int> => global(health<int>, difficulty<int>) {
        let base = health / 2
        return base * difficulty
    }
    ~calcGold<int> => global(health<int>) {
        return Math.round(health / 4)
    }
    ~createEnemy<object> => global(name<string>, health<int>, damage<int>, difficulty<int>) {
        return {
            name = name, health = health, damage = damage,
            armor = health / 10, xp = calcXp(health, difficulty), gold = calcGold(health)
        }
    }
)
@DATA( x = 1 )"#;

// =============================================================================
// Benchmark 1 — Resolver microbench
// =============================================================================

fn bench_resolver_microbench(c: &mut Criterion) {
    let mut group = c.benchmark_group("resolver_microbench");
    group.measurement_time(Duration::from_secs(8));
    group.sample_size(200);

    let (resolver_5, sample_exprs) = build_resolver(5);

    let type_labels = [
        "local_enum_access",
        "local_enum_access_alt",
        "imported_enum_access",
        "imported_func_call",
        "static_method_call",
        "static_method_call_alt",
        "object_property_access",
        "instance_method_call",
    ];

    for (i, &label) in type_labels.iter().enumerate() {
        let expr = &sample_exprs[i];
        group.throughput(Throughput::Elements(1));
        group.bench_function(label, |b| {
            b.iter(|| black_box(resolver_5.resolve_expression(black_box(expr))));
        });
    }

    {
        let no_hit = Expression::QualifiedIdentifier {
            parts:     vec!["mystery".to_string(), "field".to_string()],
            arguments: None,
            position:  Position::new(9_999_999, 1),
        };
        group.throughput(Throughput::Elements(1));
        group.bench_function("no_hit_fallback_property", |b| {
            b.iter(|| black_box(resolver_5.resolve_expression(black_box(&no_hit))));
        });
    }

    {
        let no_hit_call = Expression::QualifiedIdentifier {
            parts:     vec!["mystery".to_string(), "doThing".to_string()],
            arguments: Some(vec![]),
            position:  Position::new(9_999_998, 1),
        };
        group.throughput(Throughput::Elements(1));
        group.bench_function("no_hit_fallback_call", |b| {
            b.iter(|| black_box(resolver_5.resolve_expression(black_box(&no_hit_call))));
        });
    }

    {
        let return_stmt = QuickFuncStatement::Return {
            value:    sample_exprs[0].clone(),
            position: Position::UNKNOWN,
        };
        group.throughput(Throughput::Elements(1));
        group.bench_function("statement_return_one_qi", |b| {
            b.iter(|| black_box(resolver_5.resolve_statement(black_box(&return_stmt))));
        });
    }

    {
        let decl_stmt = QuickFuncStatement::VariableDeclaration {
            declaration_type: DeclarationType::Let,
            is_mutable:       false,
            variable_name:    "v".to_string(),
            data_type:        None,
            value:            sample_exprs[4].clone(),
            position:         Position::UNKNOWN,
        };
        group.throughput(Throughput::Elements(1));
        group.bench_function("statement_let_one_qi", |b| {
            b.iter(|| black_box(resolver_5.resolve_statement(black_box(&decl_stmt))));
        });
    }

    {
        let pos_cond  = Position::new(8_000_001, 1);
        let pos_true  = Position::new(8_000_002, 1);
        let pos_false = Position::new(8_000_003, 1);

        let mut deep_map: HashMap<QualifiedIdentifierKey, QualifiedIdentifierResolution> =
            HashMap::new();
        for (qi_idx, pos) in [(0usize, pos_cond), (4, pos_true), (6, pos_false)] {
            let (_, rt, parts, is_call) = make_qi_at(qi_idx, pos);
            deep_map.insert(
                QualifiedIdentifierKey { position: pos, parts: parts.clone(), is_call },
                QualifiedIdentifierResolution {
                    resolved_type: rt, context: None, parts, is_call, position: pos,
                },
            );
        }
        let deep_resolver = QualifiedIdentifierResolver::new(deep_map);

        let (cond_expr, _, _, _)  = make_qi_at(0, pos_cond);
        let (true_expr, _, _, _)  = make_qi_at(4, pos_true);
        let (false_expr, _, _, _) = make_qi_at(6, pos_false);

        let conditional = Expression::Conditional {
            condition:   Box::new(cond_expr),
            true_value:  Box::new(true_expr),
            false_value: Box::new(false_expr),
            position:    Position::UNKNOWN,
        };
        group.throughput(Throughput::Elements(3));
        group.bench_function("deep_conditional_3qi", |b| {
            b.iter(|| {
                black_box(deep_resolver.resolve_expression(black_box(&conditional)))
            });
        });
    }

    for &entries_per_type in &[1usize, 10, 50, 200] {
        let (sized_resolver, sized_exprs) = build_resolver(entries_per_type);
        let probe = &sized_exprs[0];
        group.throughput(Throughput::Elements(1));
        group.bench_with_input(
            BenchmarkId::new("map_size_hit", entries_per_type * 8),
            probe,
            |b, expr| {
                b.iter(|| black_box(sized_resolver.resolve_expression(black_box(expr))));
            },
        );
    }

    group.finish();
}

// =============================================================================
// Benchmark 2 — Enhancer: QI density
// =============================================================================

fn bench_enhancer_qi_density(c: &mut Criterion) {
    let mut group = c.benchmark_group("enhancer_qi_density");
    group.measurement_time(Duration::from_secs(10));
    group.sample_size(100);

    let settings = OperationalSettings::default();

    for &(qi_per_func, label) in QI_DENSITY_CASES {
        let (section, analysis_result) =
            build_section(DENSITY_FIXED_FUNC_COUNT, qi_per_func, 0);
        let total_qi = (DENSITY_FIXED_FUNC_COUNT * qi_per_func).max(1);

        group.throughput(Throughput::Elements(total_qi as u64));
        group.bench_function(label, |b| {
            let mut enhancer = QuickFunctionsAstEnhancer::new(settings.clone());
            b.iter(|| {
                black_box(enhancer.enhance(
                    black_box(&section),
                    Some(black_box(&analysis_result)),
                ))
            });
        });
    }

    for &(qi_per_func, label) in QI_DENSITY_CASES {
        let (section, _) = build_section(DENSITY_FIXED_FUNC_COUNT, qi_per_func, 500_000);

        group.bench_function(&format!("{}_no_analysis", label), |b| {
            let mut enhancer = QuickFunctionsAstEnhancer::new(settings.clone());
            b.iter(|| {
                black_box(enhancer.enhance(black_box(&section), None))
            });
        });
    }

    group.finish();
}

// =============================================================================
// Benchmark 3 — Enhancer: function count scaling
// =============================================================================

fn bench_enhancer_function_count(c: &mut Criterion) {
    let mut group = c.benchmark_group("enhancer_function_count");
    group.measurement_time(Duration::from_secs(10));
    group.sample_size(80);

    let settings = OperationalSettings::default();

    for &(func_count, label) in FUNC_COUNT_CASES {
        let (section, analysis_result) =
            build_section(func_count, SCALING_FIXED_QI_PER_FUNC, func_count * 50_000);
        let total_qi = func_count * SCALING_FIXED_QI_PER_FUNC;

        group.throughput(Throughput::Elements(total_qi as u64));
        group.bench_function(label, |b| {
            let mut enhancer = QuickFunctionsAstEnhancer::new(settings.clone());
            b.iter(|| {
                black_box(enhancer.enhance(
                    black_box(&section),
                    Some(black_box(&analysis_result)),
                ))
            });
        });
    }

    group.finish();
}

// =============================================================================
// Benchmark 4 — Object access
// =============================================================================

fn bench_object_access(c: &mut Criterion) {
    let mut group = c.benchmark_group("object_access");
    group.measurement_time(Duration::from_secs(8));
    group.sample_size(200);

    // ── Single expression resolution at varying chain depth ──────────────────

    for &(depth, label) in OBJECT_DEPTH_CASES {
        let pos = Position::new(1_000_000 + depth, 1);
        let (resolver, expr) = build_object_property_resolver(depth, pos);

        group.throughput(Throughput::Elements(1));
        group.bench_function(label, |b| {
            b.iter(|| black_box(resolver.resolve_expression(black_box(&expr))));
        });
    }

    // ── Property access vs method call — head-to-head ────────────────────────

    {
        let prop_pos = Position::new(2_000_001, 1);
        let (prop_resolver, prop_expr) = build_object_property_resolver(2, prop_pos);
        group.throughput(Throughput::Elements(1));
        group.bench_function("property_access_2part", |b| {
            b.iter(|| black_box(prop_resolver.resolve_expression(black_box(&prop_expr))));
        });
    }

    {
        let method_pos = Position::new(2_000_002, 1);
        let (method_resolver, method_expr) = build_object_method_resolver(method_pos);
        group.throughput(Throughput::Elements(1));
        group.bench_function("method_call_2part", |b| {
            b.iter(|| black_box(method_resolver.resolve_expression(black_box(&method_expr))));
        });
    }

    // ── No-hit fallback — object property path absent from map ───────────────

    {
        let no_hit_pos = Position::new(2_999_999, 1);
        let (resolver, _) = build_object_property_resolver(2, no_hit_pos);
        // Different position — guaranteed miss.
        let miss_expr = make_object_property(
            vec!["ghost".to_string(), "missing".to_string()],
            Position::new(3_000_000, 1),
        );
        group.throughput(Throughput::Elements(1));
        group.bench_function("no_hit_object_property", |b| {
            b.iter(|| black_box(resolver.resolve_expression(black_box(&miss_expr))));
        });
    }

    // ── Statement-level: return containing an object property access ─────────

    {
        let ret_pos = Position::new(3_100_001, 1);
        let (resolver, expr) = build_object_property_resolver(2, ret_pos);
        let return_stmt = QuickFuncStatement::Return {
            value: expr,
            position: Position::UNKNOWN,
        };
        group.throughput(Throughput::Elements(1));
        group.bench_function("statement_return_object_property", |b| {
            b.iter(|| black_box(resolver.resolve_statement(black_box(&return_stmt))));
        });
    }

    // ── Statement-level: let containing an object method call ────────────────

    {
        let let_pos = Position::new(3_200_001, 1);
        let (resolver, expr) = build_object_method_resolver(let_pos);
        let let_stmt = QuickFuncStatement::VariableDeclaration {
            declaration_type: DeclarationType::Let,
            is_mutable: false,
            variable_name: "result".to_string(),
            data_type: None,
            value: expr,
            position: Position::UNKNOWN,
        };
        group.throughput(Throughput::Elements(1));
        group.bench_function("statement_let_object_method", |b| {
            b.iter(|| black_box(resolver.resolve_statement(black_box(&let_stmt))));
        });
    }

    // ── Enhancer: varying object count, fixed 3 properties each ─────────────

    let settings = OperationalSettings::default();
    for &(obj_count, label) in OBJECT_COUNT_CASES {
        let total_qi = obj_count * OBJECT_FIXED_PROPS_PER_OBJ;
        let (section, analysis) =
            build_object_access_section(obj_count, OBJECT_FIXED_PROPS_PER_OBJ, obj_count * 10_000 + 4_000_000);

        group.throughput(Throughput::Elements(total_qi as u64));
        group.bench_function(label, |b| {
            let mut enhancer = QuickFunctionsAstEnhancer::new(settings.clone());
            b.iter(|| {
                black_box(enhancer.enhance(
                    black_box(&section),
                    Some(black_box(&analysis)),
                ))
            });
        });
    }

    // ── Enhancer: mixed property + method calls per object ───────────────────
    // Compare against property-only at equivalent QI count.
    // 5 objects × 2 QIs each = 10 total, same as 5obj × 2prop for fair comparison.

    for obj_count in [1usize, 5, 10] {
        let total_qi = obj_count * 2;
        let (section, analysis) =
            build_mixed_object_access_section(obj_count, obj_count * 10_000 + 5_000_000);

        group.throughput(Throughput::Elements(total_qi as u64));
        group.bench_function(
            &format!("mixed_prop_and_method_{}obj", obj_count),
            |b| {
                let mut enhancer = QuickFunctionsAstEnhancer::new(settings.clone());
                b.iter(|| {
                    black_box(enhancer.enhance(
                        black_box(&section),
                        Some(black_box(&analysis)),
                    ))
                });
            },
        );
    }

    // ── Map-size sensitivity scoped to ObjectPropertyAccess hits ─────────────
    // Adds extra non-object entries to the map to test lookup isolation.

    for &extra_entries in &[0usize, 20, 100] {
        let obj_pos = Position::new(6_000_001, 1);
        let obj_parts = vec!["target".to_string(), "field".to_string()];
        let obj_expr = make_object_property(obj_parts.clone(), obj_pos);

        let mut map: HashMap<QualifiedIdentifierKey, QualifiedIdentifierResolution> =
            HashMap::with_capacity(1 + extra_entries);

        map.insert(
            QualifiedIdentifierKey { position: obj_pos, parts: obj_parts.clone(), is_call: false },
            QualifiedIdentifierResolution {
                resolved_type: QualifiedIdentifierType::ObjectPropertyAccess,
                context: None,
                parts: obj_parts,
                is_call: false,
                position: obj_pos,
            },
        );

        // Fill with enum-type noise entries at distinct positions.
        for noise_idx in 0..extra_entries {
            let noise_pos = Position::new(6_100_000 + noise_idx + 1, 1);
            let noise_parts = vec!["Status".to_string(), format!("FIELD_{}", noise_idx)];
            map.insert(
                QualifiedIdentifierKey {
                    position: noise_pos,
                    parts: noise_parts.clone(),
                    is_call: false,
                },
                QualifiedIdentifierResolution {
                    resolved_type: QualifiedIdentifierType::LocalEnumAccess,
                    context: None,
                    parts: noise_parts,
                    is_call: false,
                    position: noise_pos,
                },
            );
        }

        let noisy_resolver = QualifiedIdentifierResolver::new(map);
        group.throughput(Throughput::Elements(1));
        group.bench_with_input(
            BenchmarkId::new("obj_hit_with_noise", 1 + extra_entries),
            &obj_expr,
            |b, expr| {
                b.iter(|| black_box(noisy_resolver.resolve_expression(black_box(expr))));
            },
        );
    }

    group.finish();
}

// =============================================================================
// Benchmark 5 — Full enhancement pipeline
// =============================================================================

fn bench_full_enhancement_pipeline(c: &mut Criterion) {
    let mut group = c.benchmark_group("full_enhancement_pipeline");
    group.measurement_time(Duration::from_secs(10));
    group.sample_size(80);

    {
        let (ast, settings, semantic_result) = parse_and_analyze(SMALL_SOURCE);
        group.throughput(Throughput::Bytes(SMALL_SOURCE.len() as u64));
        group.bench_function("enhancement_only_small", |b| {
            b.iter_batched(
                || GeneralAstEnhancer::new(&settings),
                |enhancer| {
                    black_box(enhancer.enhance(black_box(&ast), Some(&semantic_result)))
                },
                BatchSize::SmallInput,
            );
        });
    }

    {
        let (ast, settings, semantic_result) = parse_and_analyze(MEDIUM_SOURCE);
        group.throughput(Throughput::Bytes(MEDIUM_SOURCE.len() as u64));
        group.bench_function("enhancement_only_medium", |b| {
            b.iter_batched(
                || GeneralAstEnhancer::new(&settings),
                |enhancer| {
                    black_box(enhancer.enhance(black_box(&ast), Some(&semantic_result)))
                },
                BatchSize::SmallInput,
            );
        });
    }

    group.throughput(Throughput::Bytes(SMALL_SOURCE.len() as u64));
    group.bench_function("full_pipeline_small", |b| {
        b.iter(|| {
            let mut handler = ConfigSectionHandler::new(None);
            let cfg = handler.process_config_section(black_box(SMALL_SOURCE));
            let s = cfg.operational_settings.clone();
            let toks = Tokenizer::new(&cfg.cleaned_input_string, &s).tokenize();
            let parser = GeneralParser::new(toks.tokens, &cfg.config_section, &s)
                .expect("parser init");
            let ast = parser.parse().expect("parse failed");
            let semantic_result = GeneralSemanticAnalyzer::new(&ast, &s).analyze();
            let enhancer = GeneralAstEnhancer::new(&s);
            black_box(enhancer.enhance(&ast, Some(&semantic_result)))
        });
    });

    group.throughput(Throughput::Bytes(MEDIUM_SOURCE.len() as u64));
    group.bench_function("full_pipeline_medium", |b| {
        b.iter(|| {
            let mut handler = ConfigSectionHandler::new(None);
            let cfg = handler.process_config_section(black_box(MEDIUM_SOURCE));
            let s = cfg.operational_settings.clone();
            let toks = Tokenizer::new(&cfg.cleaned_input_string, &s).tokenize();
            let parser = GeneralParser::new(toks.tokens, &cfg.config_section, &s)
                .expect("parser init");
            let ast = parser.parse().expect("parse failed");
            let semantic_result = GeneralSemanticAnalyzer::new(&ast, &s).analyze();
            let enhancer = GeneralAstEnhancer::new(&s);
            black_box(enhancer.enhance(&ast, Some(&semantic_result)))
        });
    });

    if let Ok(real_src) =
        std::fs::read_to_string("mdix_files/advanced/all_datatypes_test.mdix")
    {
        let (real_ast, real_settings, real_semantic) = parse_and_analyze(&real_src);

        group.throughput(Throughput::Bytes(real_src.len() as u64));
        group.bench_function("real_file_enhancement_only", |b| {
            b.iter_batched(
                || GeneralAstEnhancer::new(&real_settings),
                |enhancer| {
                    black_box(enhancer.enhance(black_box(&real_ast), Some(&real_semantic)))
                },
                BatchSize::SmallInput,
            );
        });

        group.bench_function("real_file_full_pipeline", |b| {
            b.iter(|| {
                let mut handler = ConfigSectionHandler::new(None);
                let cfg = handler.process_config_section(black_box(&real_src));
                let s = cfg.operational_settings.clone();
                let toks = Tokenizer::new(&cfg.cleaned_input_string, &s).tokenize();
                let parser = GeneralParser::new(toks.tokens, &cfg.config_section, &s)
                    .expect("parser init");
                let ast = parser.parse().expect("parse failed");
                let semantic_result = GeneralSemanticAnalyzer::new(&ast, &s).analyze();
                let enhancer = GeneralAstEnhancer::new(&s);
                black_box(enhancer.enhance(&ast, Some(&semantic_result)))
            });
        });
    }

    group.finish();
}

// =============================================================================
// Registration
// =============================================================================

criterion_group!(
    benches,
    bench_resolver_microbench,
    bench_enhancer_qi_density,
    bench_enhancer_function_count,
    bench_object_access,
    bench_full_enhancement_pipeline,
);
criterion_main!(benches);
