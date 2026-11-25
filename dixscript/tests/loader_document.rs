// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/compiler.md, section "tests/loader_document.rs"
// ============================================================================
//! Tests for the loader returning the arena `Document` as the product of the compile pipeline.
//!
//! `DixLoader::compile_to_document` and `compile_to_document_from_str` return the `Document`
//! the pipeline built: the enhanced `@QUICKFUNCS` section, the resolved `@DATA`, the imported
//! enums the data referenced, homogenized numeric arrays and a validated `@SCHEMA`. The owned
//! entry points `compile_to_resolved_ast` and `compile_to_resolved_ast_from_str` are the same
//! pipeline plus one export. The tests check five things:
//!
//! * the owned tree the loader returns is the one the loader's stages produced before the
//!   document became the product, for every crafted source through the path entry and the
//!   string entry, errors included. The reference (`legacy_compile`) is those stages written
//!   out with the pieces that are still public: the owned enhancer output, the resolver run
//!   with `with_document`, the owned homogenizer and `validate_resolved`. The comparison drops
//!   the `created` stamp and sorts the config entries, which are clock and hash order;
//! * the document is what is exported: its enhanced functions are not the parsed ones, and
//!   literal arrays come back homogenized;
//! * the resolver in product mode (`ValueResolver::with_document_product`) leaves in the
//!   document exactly what a default run puts in its owned result, returns no owned tree and
//!   reports the same errors;
//! * the path entry and the string entry report the same input errors as before;
//! * every stage that can fail still fails with the original message.

use std::sync::MutexGuard;

use dixscript::Compiler::AST::arena::Document;
use dixscript::Compiler::AST::data_types::{CompatibilityMode, ErrorHandlingStrategy};
use dixscript::Compiler::AST::{DataEntry, DebugMode, DixScript, Value};
use dixscript::Compiler::Core::Config::{ConfigSectionHandler, OperationalSettings};
use dixscript::Compiler::Core::SectionAnalyzers::SchemaSectionAnalyzer;
use dixscript::Compiler::Core::Tokenizer::{split_config_tokens, Tokenizer};
use dixscript::Compiler::Core::ValueResolution::ValueResolver;
use dixscript::Compiler::Core::{GeneralAstEnhancer, GeneralParser, GeneralSemanticAnalyzer};
use dixscript::Compiler::Utilities::SymbolTable;
use dixscript::ErrorManager::ErrorManager;
use dixscript::Runtime::{homogenize_data_section, DixLoader};

// ==================== helpers ====================

const BASE_TYPES: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/../mdix_files/tests/imports/base_types.mdix");
const ENUM_ONLY_TYPES: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/../mdix_files/tests/imports/enum_only_types.mdix");

const HEADER: &str = "@CONFIG(\n  version -> \"1.0.0\"\n  features -> \"advanced\"\n)\n\n";
const ENUMS: &str = "@ENUMS(\n  Color { RED = 1, GREEN = 2, BLUE = 3 }\n  Level { LOW = 1, MID = 5, HIGH = 10 }\n)\n\n";

fn imports() -> String {
    format!("@IMPORTS(\n  Base from \"{BASE_TYPES}\"\n  Types from \"{ENUM_ONLY_TYPES}\"\n)\n\n")
}

fn registry_lock() -> MutexGuard<'static, ()> {
    dixscript::Builtins::Static::enum_object::ENUM_REGISTRY_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Everything the loader has when it reaches value resolution.
struct Prepared {
    document: Document,
    enhanced: DixScript,
    symbols: SymbolTable,
    settings: OperationalSettings,
    semantic_ok: bool,
}

/// The loader's stages 1 to 4 on `source`, the contents of the file at `path`: the document and
/// the settings and error manager it was parsed with. With `lenient` the stages go on after a
/// semantic error, as the error strategy `Continue` does, so a source with a bad enum reference
/// still reaches the resolver; the loader itself stops at the error.
fn parse_only(
    path: &str,
    source: &[u8],
    lenient: bool,
) -> Result<(Document, OperationalSettings, ErrorManager), String> {
    let em = ErrorManager::new_isolated_silent();
    let initial = OperationalSettings {
        source_file_path: Some(path.to_string()),
        compatibility_mode: CompatibilityMode::Strict,
        ..OperationalSettings::default()
    };
    let tokens = Tokenizer::new_with_error_manager_from_bytes(source, &initial, em.clone()).tokenize();
    let split = split_config_tokens(tokens.tokens);
    let mut handler = ConfigSectionHandler::new_with_error_manager(None, em.clone());
    let config = handler.process_config_tokens(&split.config_tokens);
    let mut settings = config.operational_settings;
    settings.source_file_path = Some(path.to_string());
    if lenient {
        settings.error_handling_strategy = ErrorHandlingStrategy::Continue;
        settings.debug_mode = DebugMode::Off;
    }
    em.update_settings(settings.clone());
    let document =
        GeneralParser::new_for_lsp(split.rest_tokens, &config.config_section, &settings, em.clone())
            .map_err(|e| format!("Parser init failed: {}", e.message()))?
            .parse_document()
            .map_err(|e| format!("Parse failed: {}", e.message()))?;
    Ok((document, settings, em))
}

/// The loader's stages 1 to 6 on `source`, as the loader ran them before the document was the
/// product: the owned tree exported from the parse is analyzed and enhanced next to the document.
/// With `lenient` see [`parse_only`].
fn prepare(path: &str, source: &[u8], lenient: bool) -> Result<Prepared, String> {
    let (mut document, settings, em) = parse_only(path, source, lenient)?;
    let ast = document.export();
    let semantic = GeneralSemanticAnalyzer::new_with_error_manager(&ast, &settings, em.clone())
        .with_document(&document)
        .analyze();
    let semantic_ok = semantic.is_success;
    if !semantic_ok && !lenient {
        let msgs: Vec<String> = semantic.errors.iter().map(|e| e.message.clone()).collect();
        return Err(format!("Semantic analysis failed: {msgs:?}"));
    }
    let enhanced = GeneralAstEnhancer::new_with_error_manager(&settings, em)
        .with_document(&mut document)
        .enhance(&ast, Some(&semantic))
        .enhanced_ast;
    let symbols = semantic.symbol_table.expect("analysis leaves a symbol table");
    Ok(Prepared { document, enhanced, symbols, settings, semantic_ok })
}

/// The loader's stages as they were before the document became the product: the owned
/// enhancer output goes through the resolver with `with_document`, whose owned result is then
/// homogenized and checked against the schema as owned trees. Holds the enum registry lock the
/// way the loader does.
fn legacy_compile(path: &str, source: &[u8]) -> Result<DixScript, String> {
    let _guard = registry_lock();
    let Prepared { mut document, enhanced, symbols, settings, .. } = prepare(path, source, false)?;
    let em = ErrorManager::new_isolated_silent();
    let mut resolved = enhanced;

    let has_functions = resolved.quick_functions.is_some()
        || symbols.namespaces.values().any(|ns| !ns.functions.is_empty());
    let has_enums = resolved.enums.as_ref().map(|e| !e.enums.is_empty()).unwrap_or(false)
        || symbols.namespaces.values().any(|ns| !ns.enums.is_empty());
    if (has_functions || has_enums) && resolved.data.is_some() {
        let result = ValueResolver::new_with_error_manager(resolved, &symbols, settings.debug_mode, em.clone())
            .with_document(&mut document)
            .resolve();
        if !result.is_success {
            return Err(format!("Value resolution failed: {:?}", result.errors));
        }
        resolved = result
            .resolved_ast
            .ok_or_else(|| "Resolution succeeded but AST is None".to_string())?;
    }

    homogenize_data_section(&mut resolved);

    if let Some(schema) = resolved.schema.as_ref() {
        let mut analyzer = SchemaSectionAnalyzer::new_with_error_manager(&settings, em);
        let result = analyzer.validate_resolved(schema, resolved.data.as_ref());
        if !result.is_success {
            let msgs: Vec<String> = result.errors.iter().map(|e| e.message.clone()).collect();
            return Err(format!("Schema validation failed: {msgs:?}"));
        }
    }
    Ok(resolved)
}

/// Drops the config entry holding the time of the run and puts the others in key order, which
/// are the only parts of a result that differ between two runs of the same input.
fn normalize(mut ast: DixScript) -> DixScript {
    if let Some(config) = ast.config.as_mut() {
        config.entries.retain(|entry| entry.key != "created");
        config.entries.sort_by(|a, b| a.key.cmp(&b.key));
    }
    ast
}

/// The crafted sources, `(name, whether it compiles, text)`. None returns an object from a
/// function or calls a random or clock builtin, whose output differs between runs.
fn sources() -> Vec<(&'static str, bool, String)> {
    let h = HEADER;
    let e = ENUMS;
    let i = imports();
    let base_only = format!("@IMPORTS(\n  Base from \"{BASE_TYPES}\"\n)\n\n");
    let mut v: Vec<(&'static str, bool, String)> = Vec::new();
    v.push(("c01_positions", true, format!("{h}{e}@DATA(\n  plain<enum> = Color.RED\n  untyped = Level.HIGH\n  list = [Color.RED, Color.GREEN]\n  grid = [[Color.RED, Color.GREEN], [Color.BLUE]]\n  plain_grid = [[1, 2], [3, 4]]\n  obj = {{ c = Color.GREEN, nested = {{ l = Level.LOW }}, arr = [Level.HIGH], plain = 1 }}\n  tup = t:(Color.RED, 1)\n  rows::\n    {{ a = Color.RED, b = [Level.LOW] }},\n    {{ a = Color.BLUE, b = [Level.HIGH] }}\n)\n")));
    v.push(("c03_calls", true, format!("{h}{e}@QUICKFUNCS(\n  ~lv<int> => global(l<enum>) {{ return l + 1 }}\n  ~two<int> => global(a<enum>, b<int>) {{ return a + b }}\n  ~arr<int> => global(xs<array>) {{ return xs.sum() }}\n)\n\n@DATA(\n  a = lv(Level.HIGH)\n  b = two(Level.LOW, 3)\n  d = arr([Level.LOW, Level.MID])\n  e = lv(lv(Level.LOW))\n  rows::\n    lv(Level.LOW),\n    two(Color.GREEN, 4)\n)\n")));
    v.push(("c04_imported", true, format!("{h}{i}@ENUMS(\n  Slot {{ HEAD = 1, CHEST = 2 }}\n)\n\n@DATA(\n  r<enum> = Base.Rarity.EPIC\n  t = Types.Priority.HIGH\n  both = [Base.ItemType.WEAPON, Slot.HEAD, Types.Region.EU]\n  o = {{ r = Base.Rarity.RARE, s = Slot.CHEST }}\n  rows::\n    {{ r = Base.Rarity.COMMON, p = Types.Priority.LOW }},\n    {{ r = Base.Rarity.LEGENDARY, p = Types.Priority.CRITICAL }}\n)\n")));
    v.push(("c05_imported_no_local_enums", true, format!("{h}{i}@DATA(\n  r = Base.Rarity.EPIC\n  p = Types.Priority.CRITICAL\n  list = [Types.Region.APAC, Types.Region.LATAM]\n)\n")));
    v.push(("c06_imported_function", true, format!("{h}{base_only}{e}@DATA(\n  local = Color.RED\n  imp = Base.ItemType.QUEST\n  scaled = Base.scaledValue(10, Base.Rarity.RARE)\n)\n")));
    v.push(("c07_no_enums_no_functions", true, format!("{h}@DATA(\n  n = 1\n  s = \"x\"\n  arr = [1, 2, 3]\n  grid = [[1, 2], [3, 4]]\n  rows::\n    {{ a = 1, b = \"x\" }},\n    {{ a = 2, b = \"y\" }}\n)\n")));
    v.push(("c09_nested_enum_only", true, format!("{h}{e}@DATA(\n  a = [[Color.RED]]\n  b = [[1], [Color.RED]]\n  d = [[Color.RED], [Color.GREEN]]\n)\n")));
    v.push(("c11_error_unknown_enum", false, format!("{h}{e}@DATA(\n  ok = Color.RED\n  bad = Nope.X\n)\n")));
    v.push(("c12_error_unknown_field", false, format!("{h}{e}@DATA(\n  ok = Color.RED\n  bad = [Level.NOPE]\n)\n")));
    v.push(("c13_error_imported", false, format!("{h}{i}@DATA(\n  ok = Base.Rarity.EPIC\n  bad = Base.Rarity.NOPE\n)\n")));
    // Numeric arrays. Literals of mixed numeric types never reach the homogenizer: semantic
    // analysis rejects them, so only a function can make an array that needs promoting.
    v.push(("m01_homogeneous_literal_arrays", true, format!("{h}@DATA(\n  weights = [1, 2, 3]\n  half = [1.5, 2.5]\n  grid = [[1, 2], [3, 4]]\n  rows::\n    [1, 2],\n    [3, 4]\n)\n")));
    // Two functions of one name. Semantic analysis refuses it; the resolver, reached through the
    // lenient stages, calls the first.
    v.push(("m10_duplicate_function_names", false, format!("{h}@QUICKFUNCS(\n  ~f<int> => global(x<int>) {{ return x + 1 }}\n  ~f<int> => global(x<int>) {{ return x + 100 }}\n)\n\n@DATA(\n  a = f(1)\n)\n")));
    v.push(("m09_mixed_literal_array", false, format!("{h}@DATA(\n  weights = [1, 2.5, 3]\n)\n")));
    v.push(("m02_computed_arrays", true, format!("{h}@QUICKFUNCS(\n  ~mix<array> => global(x<int>) {{ return [x, 2.5, x + 1] }}\n  ~ints<array> => global(x<int>) {{ return [x, x + 1] }}\n)\n\n@DATA(\n  a = mix(3)\n  b = ints(4)\n  c = [mix(1), ints(2)]\n)\n")));
    // A schema checked against the resolved data: satisfied, and violated by a computed value.
    v.push(("m03_schema_satisfied", true, format!("{h}@QUICKFUNCS(\n  ~f<int> => global(x<int>) {{ return x + 1 }}\n)\n\n@DATA(\n  n = f(1)\n)\n\n@SCHEMA(\n  n = Schema.Int(true)\n)\n")));
    v.push(("m04_schema_violated_by_a_computed_value", false, format!("{h}@QUICKFUNCS(\n  ~f<int> => global(x<int>) {{ return x + 1 }}\n)\n\n@DATA(\n  n = f(1)\n)\n\n@SCHEMA(\n  n = Schema.String(true)\n)\n")));
    v.push(("m05_functions_but_no_data", true, format!("{h}@QUICKFUNCS(\n  ~f<int> => global(x<int>) {{ return x + 1 }}\n)\n")));
    // A section the config does not enable is refused when the document is parsed.
    // A section the stand-in comparison needs a source for: @DLM that is parsed and analyzed.
    v.push(("m11_dlm_section", true, format!("{h}@DLM(\n  DCompressor.gzip\n)\n\n@DATA(\n  a = 1\n)\n")));
    v.push(("m06_parse_error", false, "@CONFIG(\n  version -> \"1.0.0\"\n  features -> \"basic\"\n)\n\n@DLM(\n  DCompressor.gzip\n)\n\n@DATA(\n  a = 1\n)\n".to_string()));
    v.push(("m07_unknown_function", false, format!("{h}@DATA(\n  a = nope(1)\n)\n")));
    // Passes semantic analysis and fails in value resolution: each call needs the other's result.
    v.push(("m08_circular_calls", false, format!("{h}@QUICKFUNCS(\n  ~f<int> => global(x<int>) {{ return x }}\n)\n\n@DATA(\n  a = f(b)\n  b = f(a)\n)\n")));
    v
}

/// A source's text written to a file in the system temporary directory, so the path entry has a
/// file to read. Returns its path.
fn write_source(name: &str, text: &str) -> String {
    let dir = std::env::temp_dir().join(format!("dix_loader_document_{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("the directory is created");
    let path = dir.join(format!("{name}.mdix"));
    std::fs::write(&path, text).expect("the source is written");
    path.to_string_lossy().into_owned()
}

fn describe(result: &Result<DixScript, String>) -> String {
    match result {
        Ok(ast) => format!("ok: {ast:?}"),
        Err(e) => format!("err: {e}"),
    }
}

/// Prints the outcome of every crafted source, for choosing and checking their expectations.
#[test]
#[ignore]
fn print_outcomes() {
    let loader = DixLoader::new_silent();
    for (name, expect_ok, text) in sources() {
        let path = write_source(name, &text);
        let outcome = match loader.compile_to_document(&path) {
            Ok(_) => "ok".to_string(),
            Err(e) => format!("err: {}", e.chars().take(110).collect::<String>()),
        };
        println!("OUTCOME {name} (expected ok: {expect_ok}): {outcome}");
    }
}

// ==================== the product equals the original stages ====================

#[test]
fn the_owned_entry_points_return_what_the_original_stages_produced() {
    let loader = DixLoader::new_silent();
    let mut compiled = 0;
    let mut failed = 0;
    for (name, expect_ok, text) in sources() {
        let path = write_source(name, &text);

        let by_path = loader.compile_to_resolved_ast(&path).map(normalize);
        let expected = legacy_compile(&path, text.as_bytes()).map(normalize);
        assert_eq!(by_path.is_ok(), expect_ok, "{name}: {:?}", by_path.as_ref().err());
        assert_eq!(describe(&by_path), describe(&expected), "{name}: path entry");

        let by_str = loader.compile_to_resolved_ast_from_str(&text, &path).map(normalize);
        assert_eq!(describe(&by_str), describe(&expected), "{name}: string entry");

        let by_document = loader.compile_to_document(&path).map(|document| normalize(document.export()));
        assert_eq!(describe(&by_document), describe(&expected), "{name}: document entry");
        let by_document_str = loader
            .compile_to_document_from_str(&text, &path)
            .map(|document| normalize(document.export()));
        assert_eq!(describe(&by_document_str), describe(&expected), "{name}: document string entry");

        if expect_ok { compiled += 1 } else { failed += 1 }
    }
    assert_eq!((compiled, failed), (12, 9), "the sources cover both outcomes");
}

#[test]
fn every_stage_that_can_fail_reports_its_original_message() {
    let loader = DixLoader::new_silent();
    let message = |name: &str| {
        let (_, _, text) = sources().into_iter().find(|(n, _, _)| *n == name).expect("a source");
        let path = write_source(name, &text);
        loader.compile_to_document(&path).err().expect("the source fails")
    };
    assert!(message("m06_parse_error").starts_with("Parse failed: "), "{}", message("m06_parse_error"));
    assert!(message("m07_unknown_function").starts_with("Semantic analysis failed: "));
    assert!(message("c11_error_unknown_enum").starts_with("Semantic analysis failed: "));
    assert!(message("m09_mixed_literal_array").starts_with("Semantic analysis failed: "));
    assert!(message("m08_circular_calls").starts_with("Value resolution failed: "), "{}", message("m08_circular_calls"));
    assert!(message("m04_schema_violated_by_a_computed_value").starts_with("Schema validation failed: "));
}

// ==================== the document is what is exported ====================

#[test]
fn the_compiled_document_holds_the_enhanced_functions_not_the_parsed_ones() {
    let (_, _, text) = sources().into_iter().find(|(n, _, _)| *n == "c03_calls").unwrap();
    let path = write_source("enhanced_not_parsed", &text);
    let prepared = prepare(&path, text.as_bytes(), false).expect("the stages run");
    assert!(prepared.semantic_ok);

    let parsed = {
        let em = ErrorManager::new_isolated_silent();
        let initial = OperationalSettings { source_file_path: Some(path.clone()), ..OperationalSettings::default() };
        let tokens = Tokenizer::new_with_error_manager_from_bytes(text.as_bytes(), &initial, em.clone()).tokenize();
        let split = split_config_tokens(tokens.tokens);
        let mut handler = ConfigSectionHandler::new_with_error_manager(None, em.clone());
        let config = handler.process_config_tokens(&split.config_tokens);
        GeneralParser::new_for_lsp(split.rest_tokens, &config.config_section, &config.operational_settings, em)
            .expect("the parser starts")
            .parse_document()
            .expect("the source parses")
            .export()
    };
    let compiled = DixLoader::new_silent().compile_to_document(&path).expect("the source compiles");
    assert_ne!(
        compiled.export().quick_functions, parsed.quick_functions,
        "the loader's document keeps the section the enhancer produced"
    );
    assert_eq!(
        compiled.export().quick_functions, prepared.enhanced.quick_functions,
        "and it is the section the owned enhancer output carries"
    );
}

fn entry_value<'a>(ast: &'a DixScript, name: &str) -> &'a Value {
    ast.data
        .as_ref()
        .expect("a data section")
        .entries
        .iter()
        .find_map(|entry| match entry {
            DataEntry::SimpleProperty { name: n, value, .. } if n == name => Some(value),
            _ => None,
        })
        .expect("a simple property of that name")
}

#[test]
fn numeric_arrays_come_back_from_the_document_homogenized() {
    let compile = |name: &str| {
        let (_, _, text) = sources().into_iter().find(|(n, _, _)| *n == name).unwrap();
        DixLoader::new_silent()
            .compile_to_document_from_str(&text, name)
            .expect("the source compiles")
            .export()
    };
    let all = |value: &Value, test: fn(&Value) -> bool| match value {
        Value::Array { values, .. } => values.iter().all(test),
        other => panic!("expected an array, got {other:?}"),
    };

    // A function that returns integers and a double gives an array of doubles; one that returns
    // only integers is left as it is.
    let ast = compile("m02_computed_arrays");
    assert!(all(entry_value(&ast, "a"), |v| matches!(v, Value::Double { .. })), "{:?}", entry_value(&ast, "a"));
    assert!(all(entry_value(&ast, "b"), |v| matches!(v, Value::Integer { .. })), "{:?}", entry_value(&ast, "b"));

    // Literals of one type are left as they are.
    let ast = compile("m01_homogeneous_literal_arrays");
    assert!(all(entry_value(&ast, "weights"), |v| matches!(v, Value::Integer { .. })));
    assert!(all(entry_value(&ast, "half"), |v| matches!(v, Value::Double { .. })));
}

// ==================== the resolver in product mode ====================

#[test]
fn product_mode_leaves_in_the_document_what_a_default_run_puts_in_its_result() {
    let mut compared = 0;
    let mut failed_in_the_resolver = 0;
    for (name, expect_ok, text) in sources() {
        let path = write_source(name, &text);
        let _guard = registry_lock();
        // Sources that fail before value resolution have nothing to compare.
        let (Ok(mut default_run), Ok(mut product_run)) =
            (prepare(&path, text.as_bytes(), false), prepare(&path, text.as_bytes(), false))
        else {
            continue;
        };
        let em = ErrorManager::new_isolated_silent();
        let default = ValueResolver::new_with_error_manager(
            default_run.enhanced.clone(), &default_run.symbols, default_run.settings.debug_mode, em.clone(),
        )
        .with_document(&mut default_run.document)
        .resolve();
        // The product run is given an empty owned tree: it takes its functions from the document.
        let product = ValueResolver::new_with_error_manager(
            DixScript::new(), &product_run.symbols, product_run.settings.debug_mode, em,
        )
        .with_document_product(&mut product_run.document)
        .resolve();

        assert_eq!(product.is_success, default.is_success, "{name}");
        assert_eq!(product.errors, default.errors, "{name}: the same errors");
        assert!(product.resolved_ast.is_none(), "{name}: product mode returns no owned tree");
        assert_eq!(
            product.function_calls_resolved, default.function_calls_resolved,
            "{name}: the same number of calls"
        );
        if !default.is_success {
            assert!(!expect_ok, "{name}");
            assert!(default.resolved_ast.is_some(), "{name}: a default run still returns its tree");
            failed_in_the_resolver += 1;
            continue;
        }
        let expected = normalize(default.resolved_ast.expect("a default run returns its tree"));
        let in_document = normalize(product_run.document.export());
        assert_eq!(
            describe(&Ok(in_document)), describe(&Ok(expected)),
            "{name}: the document holds the default run's result"
        );
        compared += 1;
    }
    assert!(compared >= 10, "only {compared} sources reached a successful resolution");
    assert!(failed_in_the_resolver >= 1, "no source failed inside the resolver");
}

#[test]
fn a_run_that_fails_in_the_enum_phase_returns_no_tree_in_product_mode() {
    // The loader stops at these sources in semantic analysis; the lenient stages let them reach
    // the resolver, whose first phase reports the bad reference and ends the run.
    for name in ["c11_error_unknown_enum", "c12_error_unknown_field", "c13_error_imported"] {
        let (_, _, text) = sources().into_iter().find(|(n, _, _)| *n == name).unwrap();
        let path = write_source(name, &text);
        let _guard = registry_lock();
        let mut default_run = prepare(&path, text.as_bytes(), true).expect("the lenient stages run");
        let mut product_run = prepare(&path, text.as_bytes(), true).expect("the lenient stages run");
        let default = ValueResolver::new_with_error_manager(
            default_run.enhanced.clone(), &default_run.symbols, default_run.settings.debug_mode,
            ErrorManager::new_isolated_silent(),
        )
        .with_document(&mut default_run.document)
        .resolve();
        let product = ValueResolver::new_with_error_manager(
            DixScript::new(), &product_run.symbols, product_run.settings.debug_mode,
            ErrorManager::new_isolated_silent(),
        )
        .with_document_product(&mut product_run.document)
        .resolve();
        assert!(!default.is_success && !product.is_success, "{name}");
        assert_eq!(product.errors, default.errors, "{name}");
        assert!(default.resolved_ast.is_some(), "{name}: a failed default run returns its tree");
        assert!(product.resolved_ast.is_none(), "{name}: a failed product run returns none");
    }
}

#[test]
fn a_duplicated_function_name_calls_the_first_definition_in_product_mode() {
    let (_, _, text) = sources().into_iter().find(|(n, _, _)| *n == "m10_duplicate_function_names").unwrap();
    let path = write_source("m10", &text);
    let _guard = registry_lock();
    let mut default_run = prepare(&path, text.as_bytes(), true).expect("the lenient stages run");
    let mut product_run = prepare(&path, text.as_bytes(), true).expect("the lenient stages run");
    let default = ValueResolver::new_with_error_manager(
        default_run.enhanced.clone(), &default_run.symbols, default_run.settings.debug_mode,
        ErrorManager::new_isolated_silent(),
    )
    .with_document(&mut default_run.document)
    .resolve();
    let product = ValueResolver::new_with_error_manager(
        DixScript::new(), &product_run.symbols, product_run.settings.debug_mode,
        ErrorManager::new_isolated_silent(),
    )
    .with_document_product(&mut product_run.document)
    .resolve();
    assert!(default.is_success && product.is_success, "{:?} {:?}", default.errors, product.errors);
    let first = default.resolved_ast.expect("a default run returns its tree");
    assert!(
        matches!(entry_value(&first, "a"), Value::Integer { value: 2, .. }),
        "the first definition adds 1: {:?}",
        entry_value(&first, "a")
    );
    assert_eq!(
        format!("{:?}", entry_value(&product_run.document.export(), "a")),
        format!("{:?}", entry_value(&first, "a")),
        "product mode calls the same function"
    );
}

#[test]
fn a_default_run_with_a_document_still_returns_its_tree() {
    let (_, _, text) = sources().into_iter().find(|(n, _, _)| *n == "c05_imported_no_local_enums").unwrap();
    let path = write_source("default_keeps_its_tree", &text);
    let _guard = registry_lock();
    let mut prepared = prepare(&path, text.as_bytes(), false).expect("the stages run");
    let result = ValueResolver::new_with_error_manager(
        prepared.enhanced.clone(), &prepared.symbols, prepared.settings.debug_mode,
        ErrorManager::new_isolated_silent(),
    )
    .with_document(&mut prepared.document)
    .resolve();
    let ast = result.resolved_ast.expect("the default run returns its tree");
    let names: Vec<&str> = ast.enums.as_ref().expect("merged enums").enums.iter().map(|d| d.name.as_str()).collect();
    assert!(names.contains(&"Base.Rarity") && names.contains(&"Types.Priority"), "{names:?}");
    // The owned tree got the merge; the document of a default run did not.
    assert!(prepared.document.export().enums.is_none());
}

// ==================== the analyzer and the owned tree ====================

/// What an analysis reports, in a form two runs can be compared by: the verdict, every error and
/// warning without its suggestion (some suggestions list names in hash order), and what the symbol
/// table holds for each imported namespace.
fn fingerprint(result: &dixscript::Compiler::Core::SemanticAnalysisResult) -> String {
    let mut errors: Vec<String> = result
        .errors
        .iter()
        .map(|e| format!("{} {} {} {}", e.error_id, e.error_type, e.section_name, e.message))
        .collect();
    errors.sort();
    let mut warnings: Vec<String> = result
        .warnings
        .iter()
        .map(|w| format!("{} {} {}", w.warning_id, w.section_name, w.message))
        .collect();
    warnings.sort();
    let mut namespaces: Vec<String> = result
        .symbol_table
        .as_ref()
        .map(|table| {
            table
                .namespaces
                .iter()
                .map(|(alias, ns)| format!("{alias}: {} functions, {} enums", ns.functions.len(), ns.enums.len()))
                .collect()
        })
        .unwrap_or_default();
    namespaces.sort();
    format!("{} {errors:?} {warnings:?} {namespaces:?}", result.is_success)
}

/// One analysis of `document` with `ast` as the owned tree, with an error manager of its own: a
/// manager keeps the errors of an earlier analysis, so two analyses must not share one.
fn analysis_of(ast: &DixScript, document: &Document, settings: &OperationalSettings) -> String {
    let em = ErrorManager::new_isolated_silent();
    em.update_settings(settings.clone());
    fingerprint(
        &GeneralSemanticAnalyzer::new_with_error_manager(ast, settings, em)
            .with_document(document)
            .analyze(),
    )
}

#[test]
fn the_analyzer_reads_only_the_document_when_it_has_one() {
    // The loader gives the analyzer an empty owned tree and the document. That is only the same
    // as giving it the tree exported from the document if no section is still read from the tree.
    let mut analyzed = 0;
    for (name, _, text) in sources() {
        let path = write_source(name, &text);
        let _guard = registry_lock();
        for lenient in [false, true] {
            let Ok((document, settings, _)) = parse_only(&path, text.as_bytes(), lenient) else {
                continue;
            };
            let exported = document.export();
            let stand_in = DixScript::new();
            let run = |ast: &DixScript| analysis_of(ast, &document, &settings);
            assert_eq!(run(&exported), run(&stand_in), "{name} (lenient: {lenient})");
            analyzed += 1;
        }
    }
    assert!(analyzed >= 30, "only {analyzed} analyses were compared");

    // The corpus has every section the crafted sources lack: @RAW, @SECURITY, @DLM that is
    // enabled in a real file, imports of several files.
    let corpus = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../mdix_files");
    let mut files = Vec::new();
    collect_mdix(&corpus, &mut files);
    files.sort();
    let mut corpus_analyses = 0;
    for file in files {
        let Ok(bytes) = std::fs::read(&file) else { continue };
        let path = file.to_string_lossy().into_owned();
        // The cloud imports go to the network and wait for a timeout, which is slow and says
        // nothing about the sections; the big files have no section the small ones lack.
        if bytes.len() > 100_000 || path.contains("_cloud_") {
            continue;
        }
        let _guard = registry_lock();
        let Ok((document, settings, _)) = parse_only(&path, &bytes, false) else { continue };
        let exported = document.export();
        let stand_in = DixScript::new();
        let run = |ast: &DixScript| analysis_of(ast, &document, &settings);
        assert_eq!(run(&exported), run(&stand_in), "{path}");
        corpus_analyses += 1;
    }
    assert!(corpus_analyses >= 70, "only {corpus_analyses} corpus files were compared");
}

fn collect_mdix(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_mdix(&path, out);
        } else if path.extension().map_or(false, |e| e == "mdix") {
            out.push(path);
        }
    }
}

// ==================== input errors ====================

#[test]
fn the_document_entries_report_the_input_errors_of_the_owned_ones() {
    let loader = DixLoader::new_silent();
    let missing = "/no/such/directory/missing.mdix";
    let by_document = loader.compile_to_document(missing).err().expect("a missing file fails");
    let by_owned = loader.compile_to_resolved_ast(missing).err().expect("a missing file fails");
    assert_eq!(by_document, format!("File not found: {missing}"));
    assert_eq!(by_document, by_owned);

    let by_document = loader.compile_to_document_from_str("  \n\t", "blank").err().expect("a blank source fails");
    let by_owned = loader.compile_to_resolved_ast_from_str("  \n\t", "blank").err().expect("a blank source fails");
    assert_eq!(by_document, "'blank': source is empty");
    assert_eq!(by_document, by_owned);
}
