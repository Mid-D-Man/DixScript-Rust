// benches/ast_enhancement_benchmark.rs
//! AST Enhancement Benchmark — DixScript v1.0.0
//!
//! Groups:
//! 1. `resolver_microbench`      — per-type resolution cost and map-size sensitivity
//! 2. `enhancer_qi_density`      — 5 functions, varying QI count per function
//! 3. `enhancer_function_count`  — fixed 10 QIs/func, varying function count (linear scaling check)
//! 4. `object_access`            — property vs method call, depth scaling, density scaling
//! 5. `full_enhancement_pipeline`— GeneralAstEnhancer end-to-end on real source inputs

use criterion::{
    black_box, criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion, Throughput,
};
use dixscript::Compiler::AST::{
    DataType, DeclarationType, Expression, Position, QuickFunction, QuickFuncParam,
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
use dixscript::ErrorManager::{DebugConfig, DebugMode};
use std::collections::HashMap;
use std::time::Duration;

// =============================================================================
// Constants
// =============================================================================

const QI_DENSITY_CASES: &[(usize, &str)] = &[
    (0,  "0qi_baseline"),
    (5,  "5qi_per_func"),
    (15, "15qi_per_func"),
    (30, "30qi_per_func"),
];

const FUNC_COUNT_CASES: &[(usize, &str)] = &[
    (1,  "1func"),
    (10, "10funcs"),
    (25, "25funcs"),
    (50, "50funcs"),
];

const OBJECT_DEPTH_CASES: &[(usize, &str)] = &[
    (1, "depth_1_single"),
    (2, "depth_2_nested"),
    (4, "depth_4_very_deep"),
];

const OBJECT_COUNT_CASES: &[(usize, &str)] = &[
    (1,  "1obj"),
    (5,  "5objs"),
    (20, "20objs"),
];

const DENSITY_FIXED_FUNC_COUNT: usize = 5;
const SCALING_FIXED_QI_PER_FUNC: usize = 10;
const OBJECT_FIXED_PROPS_PER_OBJ: usize = 3;

fn bench_debug_config() -> DebugConfig {
    DebugConfig::from_debug_mode(DebugMode::Off)
}

// =============================================================================
// QI pattern table
// =============================================================================

fn make_qi_at(
    index: usize,
    pos: Position,
) -> (Expression, QualifiedIdentifierType, Vec<String>, bool) {
    let build = |parts: Vec<String>, is_call: bool| Expression::QualifiedIdentifier {
        parts: parts.clone(),
        arguments: if is_call { Some(vec![]) } else { None },
        position: pos,
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

fn make_object_property(parts: Vec<String>, pos: Position) -> Expression {
    Expression::QualifiedIdentifier { parts, arguments: None, position: pos }
}

fn make_object_method(parts: Vec<String>, pos: Position) -> Expression {
    Expression::QualifiedIdentifier { parts, arguments: Some(vec![]), position: pos }
}

fn build_object_property_resolver(
    depth: usize,
    pos: Position,
) -> (QualifiedIdentifierResolver, Expression) {
    assert!(depth >= 1);
    let mut parts = vec!["obj0".to_string()];
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
    (QualifiedIdentifierResolver::new(map, bench_debug_config()), expr)
}

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
    (QualifiedIdentifierResolver::new(map, bench_debug_config()), expr)
}

fn build_object_access_section(
    obj_count: usize,
    props_per_obj: usize,
    pos_base: usize,
) -> (QuickFuncsSection, SectionAnalysisResult) {
    let total_qi = obj_count * props_per_obj;
    let mut resolutions = HashMap::with_capacity(total_qi);
    let mut body = Vec::with_capacity(total_qi.max(1));

    let mut stmt_idx = 0usize;
    for obj_idx in 0..obj_count {
        for prop_idx in 0..props_per_obj {
            let pos = Position::new(pos_base + stmt_idx + 1, stmt_idx + 1);
            let parts = vec![format!("obj{}", obj_idx), format!("field{}", prop_idx)];
            let is_last = stmt_idx + 1 == total_qi;
            let expr = make_object_property(parts.clone(), pos);

            resolutions.insert(
                QualifiedIdentifierKey { position: pos, parts: parts.clone(), is_call: false },
                QualifiedIdentifierResolution {
                    resolved_type: QualifiedIdentifierType::ObjectPropertyAccess,
                    context: None,
                    parts,
                    is_call: false,
                    position: pos,
                },
            );

            if is_last {
                body.push(QuickFuncStatement::Return { value: expr, position: Position::UNKNOWN });
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

// =============================================================================
// Construction helpers
// =============================================================================

fn build_resolver(entries_per_type: usize) -> (QualifiedIdentifierResolver, Vec<Expression>) {
    let total = entries_per_type * 8;
    let mut map = HashMap::with_capacity(total);
    let mut sample_exprs = Vec::with_capacity(8);

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

    (QualifiedIdentifierResolver::new(map, bench_debug_config()), sample_exprs)
}

fn build_function(
    name: &str,
    qi_count: usize,
    pos_base: usize,
) -> (QuickFunction, HashMap<QualifiedIdentifierKey, QualifiedIdentifierResolution>) {
    let mut resolutions = HashMap::with_capacity(qi_count);
    let mut body = Vec::with_capacity(qi_count.max(1));

    for qi_idx in 0..qi_count {
        let pos = Position::new(pos_base + qi_idx + 1, qi_idx + 1);
        let (expr, resolved_type, parts, is_call) = make_qi_at(qi_idx, pos);

        resolutions.insert(
            QualifiedIdentifierKey { position: pos, parts: parts.clone(), is_call },
            QualifiedIdentifierResolution { resolved_type, context: None, parts, is_call, position: pos },
        );

        if qi_idx + 1 < qi_count {
            body.push(QuickFuncStatement::VariableDeclaration {
                declaration_type: DeclarationType::Let,
                is_mutable: false,
                variable_name: format!("v{}", qi_idx),
                data_type: None,
                value: expr,
                position: Position::UNKNOWN,
            });
        } else {
            body.push(QuickFuncStatement::Return { value: expr, position: Position::UNKNOWN });
        }
    }

    if body.is_empty() {
        body.push(QuickFuncStatement::Return {
            value: Expression::Value {
                value: Value::Integer { value: 42, position: Position::UNKNOWN },
                position: Position::UNKNOWN,
            },
            position: Position::UNKNOWN,
        });
    }

    let func = QuickFunction {
        name: name.to_string(),
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

    (func, resolutions)
}

fn build_section(
    func_count: usize,
    qi_per_func: usize,
    pos_base: usize,
) -> (QuickFuncsSection, SectionAnalysisResult) {
    let mut resolutions = HashMap::with_capacity(func_count * qi_per_func);
    let mut functions = Vec::with_capacity(func_count);

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

fn parse_and_analyze(source: &str) -> (dixscript::Compiler::AST::DixScript, OperationalSettings, SemanticAnalysisResult) {
    let mut handler = ConfigSectionHandler::new(None);
    let cfg = handler.process_config_section(source);
    let settings = cfg.operational_settings.clone();
    let toks = Tokenizer::new(&cfg.cleaned_input_string, &settings).tokenize();
    let parser = GeneralParser::new(toks.tokens, &cfg.config_section, &settings).expect("parser init");
    let ast = parser.parse().expect("parse failed");
    let semantic_result = GeneralSemanticAnalyzer::new(&ast, &settings).analyze();
    (ast, settings, semantic_result)
}

// =============================================================================
// Source inputs
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
    ~poolSize<int> => global(env<int>, base<int>) {
        let multiplier = env == 3 ? 5 : env == 2 ? 2 : 1
        return base * multiplier
    }
    ~validatePort<bool> => global(port<int>) {
        return port > 1024 && port < 65536
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
    group.measurement_time(Duration::from_secs(6));
    group.sample_size(150);

    let (resolver, sample_exprs) = build_resolver(5);

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
        group.throughput(Throughput::Elements(1));
        group.bench_function(label, |b| {
            b.iter(|| black_box(resolver.resolve_expression(black_box(&sample_exprs[i]))));
        });
    }

    let no_hit = Expression::QualifiedIdentifier {
        parts: vec!["mystery".to_string(), "field".to_string()],
        arguments: None,
        position: Position::new(9_999_999, 1),
    };
    group.throughput(Throughput::Elements(1));
    group.bench_function("no_hit_fallback", |b| {
        b.iter(|| black_box(resolver.resolve_expression(black_box(&no_hit))));
    });

    // Map-size sensitivity — 3 sizes only
    for &entries_per_type in &[1usize, 50, 200] {
        let (sized_resolver, sized_exprs) = build_resolver(entries_per_type);
        group.throughput(Throughput::Elements(1));
        group.bench_with_input(
            BenchmarkId::new("map_size_hit", entries_per_type * 8),
            &sized_exprs[0],
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
    group.measurement_time(Duration::from_secs(8));
    group.sample_size(80);

    let settings = OperationalSettings::default();

    for &(qi_per_func, label) in QI_DENSITY_CASES {
        let (section, analysis_result) =
            build_section(DENSITY_FIXED_FUNC_COUNT, qi_per_func, 0);
        let total_qi = (DENSITY_FIXED_FUNC_COUNT * qi_per_func).max(1);

        group.throughput(Throughput::Elements(total_qi as u64));
        group.bench_function(label, |b| {
            b.iter(|| {
                let mut enhancer = QuickFunctionsAstEnhancer::new(&settings);
                black_box(enhancer.enhance(black_box(&section), Some(black_box(&analysis_result))))
            });
        });
    }

    // No-analysis baseline
    let (section_no_qi, _) = build_section(DENSITY_FIXED_FUNC_COUNT, 0, 500_000);
    group.bench_function("no_analysis_baseline", |b| {
        b.iter(|| {
            let mut enhancer = QuickFunctionsAstEnhancer::new(&settings);
            black_box(enhancer.enhance(black_box(&section_no_qi), None))
        });
    });

    group.finish();
}

// =============================================================================
// Benchmark 3 — Enhancer: function count scaling
// =============================================================================

fn bench_enhancer_function_count(c: &mut Criterion) {
    let mut group = c.benchmark_group("enhancer_function_count");
    group.measurement_time(Duration::from_secs(8));
    group.sample_size(60);

    let settings = OperationalSettings::default();

    for &(func_count, label) in FUNC_COUNT_CASES {
        let (section, analysis_result) =
            build_section(func_count, SCALING_FIXED_QI_PER_FUNC, func_count * 50_000);
        let total_qi = func_count * SCALING_FIXED_QI_PER_FUNC;

        group.throughput(Throughput::Elements(total_qi as u64));
        group.bench_function(label, |b| {
            b.iter(|| {
                let mut enhancer = QuickFunctionsAstEnhancer::new(&settings);
                black_box(enhancer.enhance(black_box(&section), Some(black_box(&analysis_result))))
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
    group.measurement_time(Duration::from_secs(6));
    group.sample_size(150);

    // Depth scaling
    for &(depth, label) in OBJECT_DEPTH_CASES {
        let pos = Position::new(1_000_000 + depth, 1);
        let (resolver, expr) = build_object_property_resolver(depth, pos);
        group.throughput(Throughput::Elements(1));
        group.bench_function(label, |b| {
            b.iter(|| black_box(resolver.resolve_expression(black_box(&expr))));
        });
    }

    // Property vs method call
    {
        let pos = Position::new(2_000_001, 1);
        let (resolver, expr) = build_object_property_resolver(2, pos);
        group.throughput(Throughput::Elements(1));
        group.bench_function("property_vs_method_property", |b| {
            b.iter(|| black_box(resolver.resolve_expression(black_box(&expr))));
        });
    }
    {
        let pos = Position::new(2_000_002, 1);
        let (resolver, expr) = build_object_method_resolver(pos);
        group.throughput(Throughput::Elements(1));
        group.bench_function("property_vs_method_call", |b| {
            b.iter(|| black_box(resolver.resolve_expression(black_box(&expr))));
        });
    }

    // Enhancer: object density
    let settings = OperationalSettings::default();
    for &(obj_count, label) in OBJECT_COUNT_CASES {
        let total_qi = obj_count * OBJECT_FIXED_PROPS_PER_OBJ;
        let (section, analysis) =
            build_object_access_section(obj_count, OBJECT_FIXED_PROPS_PER_OBJ, obj_count * 10_000 + 4_000_000);

        group.throughput(Throughput::Elements(total_qi as u64));
        group.bench_function(label, |b| {
            b.iter(|| {
                let mut enhancer = QuickFunctionsAstEnhancer::new(&settings);
                black_box(enhancer.enhance(black_box(&section), Some(black_box(&analysis))))
            });
        });
    }

    group.finish();
}

// =============================================================================
// Benchmark 5 — Full enhancement pipeline
// =============================================================================

fn bench_full_enhancement_pipeline(c: &mut Criterion) {
    let mut group = c.benchmark_group("full_enhancement_pipeline");
    group.measurement_time(Duration::from_secs(8));
    group.sample_size(60);

    {
        let (ast, settings, semantic_result) = parse_and_analyze(SMALL_SOURCE);
        group.throughput(Throughput::Bytes(SMALL_SOURCE.len() as u64));
        group.bench_function("enhancement_only_small", |b| {
            b.iter_batched(
                || GeneralAstEnhancer::new(&settings),
                |enhancer| black_box(enhancer.enhance(black_box(&ast), Some(&semantic_result))),
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
                |enhancer| black_box(enhancer.enhance(black_box(&ast), Some(&semantic_result))),
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
            let parser = GeneralParser::new(toks.tokens, &cfg.config_section, &s).expect("parser init");
            let ast = parser.parse().expect("parse failed");
            let semantic_result = GeneralSemanticAnalyzer::new(&ast, &s).analyze();
            let enhancer = GeneralAstEnhancer::new(&s);
            black_box(enhancer.enhance(&ast, Some(&semantic_result)))
        });
    });

    if let Ok(real_src) = std::fs::read_to_string("mdix_files/advanced/all_datatypes_test.mdix") {
        let (real_ast, real_settings, real_semantic) = parse_and_analyze(&real_src);
        group.throughput(Throughput::Bytes(real_src.len() as u64));
        group.bench_function("real_file_enhancement_only", |b| {
            b.iter_batched(
                || GeneralAstEnhancer::new(&real_settings),
                |enhancer| black_box(enhancer.enhance(black_box(&real_ast), Some(&real_semantic))),
                BatchSize::SmallInput,
            );
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
