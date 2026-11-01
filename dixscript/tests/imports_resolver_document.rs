// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/compiler.md, section "tests/imports_resolver_document.rs"
// ============================================================================
//! Tests for `ImportsResolver` parsing the files it imports to a `Document`.
//!
//! For every imported file the resolver tokenizes, parses, analyzes and enhances it, and registers
//! its global functions and its enums as a namespace of the symbol table. It used to parse to the
//! owned tree and let the analyzer and the enhancer copy the owned sections back into scratch arenas;
//! it now parses to a `Document` and hands the document to both, which is the path the loader takes
//! for the main file. A file that the caller parsed itself (`resolve_imports`) still takes the owned
//! path. For a file the resolver parses, the document and the owned tree agree by construction, so
//! the result cannot show which path was taken; the tests pin what the result is. They check:
//!
//! * the namespaces and the imports errors of 16 crafted files (a library with global and scoped
//!   functions, defaults, a lambda and an enum, files that import it directly, twice over, and through
//!   each other, a missing, an empty, an unparsable and an unanalyzable import, a file that imports
//!   itself) are the ones the code before the change produced. `RECORDING` holds a fingerprint of each
//!   file's dump, recorded by running the original code;
//! * that is so whether the analyzer of the main file is given its own document or not;
//! * the namespaces the resolver builds from files it reads itself are the same as the ones it builds
//!   from the same files parsed by the caller, which is the old path.
//!
//! The files are written to a directory under the system temporary directory, because the resolver
//! reads imports from disk.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use dixscript::Compiler::AST::data_types::{CompatibilityMode, ErrorHandlingStrategy};
use dixscript::Compiler::AST::DixScript;
use dixscript::Compiler::Core::Config::{ConfigSectionHandler, OperationalSettings};
use dixscript::Compiler::Core::Tokenizer::{split_config_tokens, Tokenizer};
use dixscript::Compiler::Core::{GeneralParser, GeneralSemanticAnalyzer};
use dixscript::Compiler::ImportsResolution::ImportsResolver;
use dixscript::Compiler::Utilities::SymbolTable;
use dixscript::ErrorManager::ErrorManager;

// ==================== the crafted files ====================

const FILES: &[(&str, &str)] = &[
    ("lib_a.mdix", r##"@CONFIG(
  version -> "1.0.0"
  features -> "advanced"
)
@ENUMS(
  Color { RED = 0, GREEN = 1, BLUE = 5 }
)
@QUICKFUNCS(
  ~double<int> => global(n<int> = 2) {
    return n * 2
  }
  ~greet<string> => global(name<string>, loud<bool>) {
    let s = name.upper()
    return loud ? s : name
  }
  ~hidden<int> => data.users() {
    return 1
  }
  ~with_lambda<int> => global(xs<array<int>>) {
    let sq = (k) => k.abs()
    return sq(xs.first())
  }
)
@DATA(
  lib_a_marker = 1
)
"##),
    ("lib_b.mdix", r##"@CONFIG(
  version -> "1.0.0"
  features -> "advanced"
)
@IMPORTS(
  A from "lib_a.mdix"
)
@QUICKFUNCS(
  ~quad<int> => global(n<int>) {
    return A.double(A.double(n))
  }
  ~shade<int> => global() {
    return A.Color.BLUE
  }
)
@DATA(
  lib_b_marker = 2
)
"##),
    ("lib_bad_parse.mdix", r##"@CONFIG(
  version -> "1.0.0"
  features -> "advanced"
)
@QUICKFUNCS(
  ~broken<int> => global( {
"##),
    ("lib_bad_semantic.mdix", r##"@CONFIG(
  version -> "1.0.0"
  features -> "advanced"
)
@QUICKFUNCS(
  ~wrong<int> => global() {
    return "not an int"
  }
)
@DATA(
  x = 1
)
"##),
    ("lib_c.mdix", r##"@CONFIG(
  version -> "1.0.0"
  features -> "advanced"
)
@IMPORTS(
  A from "lib_a.mdix"
)
@QUICKFUNCS(
  ~triple<int> => global(n<int>) {
    return n + A.double(n)
  }
)
@DATA(
  lib_c_marker = 3
)
"##),
    ("lib_empty.mdix", r##""##),
    ("lib_self.mdix", r##"@CONFIG(
  version -> "1.0.0"
  features -> "advanced"
)
@IMPORTS(
  Me from "lib_self.mdix"
)
@DATA(
  x = 1
)
"##),
    ("main_bad_parse.mdix", r##"@CONFIG(
  version -> "1.0.0"
  features -> "advanced"
)
@IMPORTS(
  P from "lib_bad_parse.mdix"
)
@DATA(
  x = 1
)
"##),
    ("main_bad_semantic.mdix", r##"@CONFIG(
  version -> "1.0.0"
  features -> "advanced"
)
@IMPORTS(
  S from "lib_bad_semantic.mdix"
)
@DATA(
  x = 1
)
"##),
    ("main_diamond.mdix", r##"@CONFIG(
  version -> "1.0.0"
  features -> "advanced"
)
@IMPORTS(
  B from "lib_b.mdix"
  C from "lib_c.mdix"
)
@DATA(
  b = B.quad(1)
  c = C.triple(1)
)
"##),
    ("main_empty.mdix", r##"@CONFIG(
  version -> "1.0.0"
  features -> "advanced"
)
@IMPORTS(
  E from "lib_empty.mdix"
)
@DATA(
  x = 1
)
"##),
    ("main_missing.mdix", r##"@CONFIG(
  version -> "1.0.0"
  features -> "advanced"
)
@IMPORTS(
  Gone from "no_such_lib.mdix"
)
@DATA(
  x = 1
)
"##),
    ("main_mixed.mdix", r##"@CONFIG(
  version -> "1.0.0"
  features -> "advanced"
)
@IMPORTS(
  A from "lib_a.mdix"
  Gone from "no_such_lib.mdix"
  B from "lib_b.mdix"
)
@DATA(
  a = A.double(1)
  b = B.quad(1)
)
"##),
    ("main_ok.mdix", r##"@CONFIG(
  version -> "1.0.0"
  features -> "advanced"
)
@IMPORTS(
  A from "lib_a.mdix"
  B from "lib_b.mdix"
)
@QUICKFUNCS(
  ~local<int> => global() {
    let d = A.double(4)
    let q = B.quad(2)
    let c = A.Color.GREEN
    return d + q + c
  }
)
@DATA(
  direct = A.double(10)
  nested = B.quad(3)
  shade = B.shade()
  loud = A.greet("x", true)
  local = local()
)
"##),
    ("main_scoped.mdix", r##"@CONFIG(
  version -> "1.0.0"
  features -> "advanced"
)
@IMPORTS(
  A from "lib_a.mdix"
)
@DATA(
  h = A.hidden()
)
"##),
    ("main_self_cycle.mdix", r##"@CONFIG(
  version -> "1.0.0"
  features -> "advanced"
)
@IMPORTS(
  Me from "lib_self.mdix"
)
@DATA(
  x = 1
)
"##),
];

/// (file, namespaces registered, imports errors, fingerprint of the dump), recorded from the code
/// before the change.
const RECORDING: &[(&str, usize, usize, u64)] = &[
    ("lib_a.mdix", 0, 0, 0xcbf29ce484222325),
    ("lib_b.mdix", 1, 0, 0xd13ca5a6dad1aa27),
    ("lib_bad_parse.mdix", 0, 0, 0xcbf29ce484222325),
    ("lib_bad_semantic.mdix", 0, 0, 0xcbf29ce484222325),
    ("lib_c.mdix", 1, 0, 0xd13ca5a6dad1aa27),
    ("lib_empty.mdix", 0, 0, 0xcbf29ce484222325),
    ("lib_self.mdix", 0, 1, 0x626ffd8bc6b8c410),
    ("main_bad_parse.mdix", 0, 1, 0xc57789a42a4c7a60),
    ("main_bad_semantic.mdix", 0, 1, 0xa81dea25e15dc629),
    ("main_diamond.mdix", 3, 0, 0x1e5fc9dea79f6f99),
    ("main_empty.mdix", 1, 0, 0x5b7577b2ac82a500),
    ("main_missing.mdix", 0, 1, 0x0f5122d11515c43a),
    ("main_mixed.mdix", 1, 1, 0x4c98590eeec3c394),
    ("main_ok.mdix", 2, 0, 0x574505214db5efcd),
    ("main_scoped.mdix", 1, 0, 0xd13ca5a6dad1aa27),
    ("main_self_cycle.mdix", 0, 1, 0x626ffd8bc6b8c410),
];

/// Writes the files once and returns their directory.
fn tree() -> &'static PathBuf {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        let dir = std::env::temp_dir().join(format!("dixscript_imports_resolver_document_{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("a temporary directory");
        for (name, text) in FILES {
            std::fs::write(dir.join(name), text).expect("a file");
        }
        dir
    })
}

fn fnv(text: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

// ==================== running a file ====================

struct Parsed {
    ast: DixScript,
    document: dixscript::Compiler::AST::arena::Document,
    settings: OperationalSettings,
}

fn parse_file(path: &Path, em: &ErrorManager) -> Parsed {
    let label = path.to_string_lossy().to_string();
    let bytes = std::fs::read(path).expect("a readable file");
    let initial = OperationalSettings {
        source_file_path: Some(label.clone()),
        compatibility_mode: CompatibilityMode::Strict,
        ..OperationalSettings::default()
    };
    let tokens = Tokenizer::new_with_error_manager_from_bytes(&bytes, &initial, em.clone()).tokenize();
    let split = split_config_tokens(tokens.tokens);
    let mut handler = ConfigSectionHandler::new_with_error_manager(None, em.clone());
    let config = handler.process_config_tokens(&split.config_tokens);
    let mut settings = config.operational_settings;
    settings.source_file_path = Some(label);
    em.update_settings(settings.clone());
    let document = GeneralParser::new_for_lsp(split.rest_tokens, &config.config_section, &settings, em.clone())
        .expect("the parser starts")
        .parse_document()
        .expect("the file parses");
    let ast = document.export();
    Parsed { ast, document, settings }
}

/// The namespaces of the symbol table in full, one line each, and the imports errors, with the
/// directory of the files replaced by `<DIR>`. The format is the one the recording was made with.
fn dump(table: &SymbolTable, em: &ErrorManager) -> Vec<String> {
    let dir = tree().to_string_lossy().to_string();
    let canonical = std::fs::canonicalize(tree()).map(|p| p.to_string_lossy().to_string()).unwrap_or_else(|_| dir.clone());
    let mask = |s: String| s.replace(&canonical, "<DIR>").replace(&dir, "<DIR>");

    let mut lines = Vec::new();
    let namespaces: BTreeMap<_, _> = table.namespaces.iter().collect();
    for (name, ns) in namespaces {
        let functions: BTreeMap<_, _> = ns.functions.iter().map(|(n, i)| (n.clone(), format!("{i:?}"))).collect();
        let enums: BTreeMap<_, BTreeMap<_, _>> = ns
            .enums
            .iter()
            .map(|(n, m)| (n.clone(), m.iter().map(|(a, b)| (a.clone(), *b)).collect()))
            .collect();
        let locals: BTreeSet<_> = ns.local_imports.keys().cloned().collect();
        lines.push(mask(format!("SEM ns_full {name} functions={functions:?} enums={enums:?} locals={locals:?}")));
    }
    let mut errors: Vec<String> =
        em.get_imports_resolution_errors().iter().map(|e| mask(format!("IMPORT_ERROR {e:?}"))).collect();
    errors.sort();
    lines.extend(errors);
    lines
}

/// Analyzes `name` from the tree, which resolves its imports, and dumps what was registered.
fn analyze_file(name: &str, with_document: bool) -> Vec<String> {
    let em = ErrorManager::new_isolated_silent();
    let parsed = parse_file(&tree().join(name), &em);
    let mut analyzer = GeneralSemanticAnalyzer::new_with_error_manager(&parsed.ast, &parsed.settings, em.clone());
    if with_document {
        analyzer = analyzer.with_document(&parsed.document);
    }
    let result = analyzer.analyze();
    dump(&result.symbol_table.expect("a symbol table"), &em)
}

fn recorded(name: &str) -> (usize, usize, u64) {
    RECORDING
        .iter()
        .find(|(n, _, _, _)| *n == name)
        .map(|(_, ns, ie, hash)| (*ns, *ie, *hash))
        .unwrap_or_else(|| panic!("no recording for {name}"))
}

#[test]
fn every_crafted_file_has_a_recording() {
    assert_eq!(FILES.len(), RECORDING.len());
    for (name, _) in FILES {
        recorded(name);
    }
}

#[test]
fn namespaces_and_errors_are_the_recorded_ones_without_a_document_for_the_main_file() {
    for (name, _) in FILES {
        let lines = analyze_file(name, false);
        let (namespaces, errors, hash) = recorded(name);
        assert_eq!(lines.iter().filter(|l| l.starts_with("SEM ns_full ")).count(), namespaces, "{name}: namespaces");
        assert_eq!(lines.iter().filter(|l| l.starts_with("IMPORT_ERROR ")).count(), errors, "{name}: errors");
        assert_eq!(fnv(&lines.join("\n")), hash, "{name}:\n{}", lines.join("\n"));
    }
}

#[test]
fn namespaces_and_errors_are_the_recorded_ones_with_a_document_for_the_main_file() {
    for (name, _) in FILES {
        let lines = analyze_file(name, true);
        let (_, _, hash) = recorded(name);
        assert_eq!(fnv(&lines.join("\n")), hash, "{name}:\n{}", lines.join("\n"));
    }
}

// ==================== what the errors are ====================

fn error_ids(name: &str) -> Vec<String> {
    let em = ErrorManager::new_isolated_silent();
    let parsed = parse_file(&tree().join(name), &em);
    let _ = GeneralSemanticAnalyzer::new_with_error_manager(&parsed.ast, &parsed.settings, em.clone())
        .with_document(&parsed.document)
        .analyze();
    let mut ids: Vec<String> = em.get_imports_resolution_errors().iter().map(|e| e.error_id.clone()).collect();
    ids.sort();
    ids
}

#[test]
fn each_kind_of_import_failure_is_reported_once() {
    assert!(error_ids("main_ok.mdix").is_empty());
    assert!(error_ids("main_diamond.mdix").is_empty());
    assert!(error_ids("main_empty.mdix").is_empty());
    assert_eq!(error_ids("main_missing.mdix"), ["IMP001"], "file not found");
    assert_eq!(error_ids("main_self_cycle.mdix"), ["IMP002"], "circular dependency");
    assert_eq!(error_ids("main_bad_parse.mdix"), ["IMP004"], "the file does not analyze");
    assert_eq!(error_ids("main_bad_semantic.mdix"), ["IMP004"], "the file does not analyze");
    // The good imports around a bad one are still registered.
    assert_eq!(error_ids("main_mixed.mdix"), ["IMP001"]);
}

// ==================== the old path and the new one agree ====================

/// A file parsed by the caller, with its path, as `ImportsResolver::resolve_imports` takes it.
fn parsed_by_the_caller(name: &str, em: &ErrorManager) -> (String, DixScript) {
    let path = tree().join(name);
    let parsed = parse_file(&path, em);
    (path.to_string_lossy().to_string(), parsed.ast)
}

/// Resolves `aliases` (alias, file) through the caller-parsed entry point and dumps the table.
fn resolve_pre_parsed(aliases: &[(&str, &str)]) -> Vec<String> {
    let em = ErrorManager::new_isolated_silent();
    let settings = OperationalSettings { error_handling_strategy: ErrorHandlingStrategy::Continue, ..OperationalSettings::default() };
    let pre_parsed: HashMap<String, (String, DixScript)> =
        aliases.iter().map(|(alias, file)| (alias.to_string(), parsed_by_the_caller(file, &em))).collect();
    let mut table = SymbolTable::new();
    let ok = ImportsResolver::new_with_error_manager(&mut table, &settings, em.clone()).resolve_imports(&pre_parsed);
    assert!(ok, "{:?}", em.get_imports_resolution_errors());
    dump(&table, &em)
}

#[test]
fn files_parsed_by_the_caller_register_the_namespaces_the_resolver_builds_from_files_it_reads() {
    for (main, aliases) in [
        ("main_ok.mdix", vec![("A", "lib_a.mdix"), ("B", "lib_b.mdix")]),
        ("main_diamond.mdix", vec![("B", "lib_b.mdix"), ("C", "lib_c.mdix")]),
    ] {
        let by_the_resolver = analyze_file(main, true);
        let by_the_caller = resolve_pre_parsed(&aliases);
        // The caller-parsed path registers the libraries under the aliases it was given, and the
        // dependencies of each under the aliases its own file uses, so for these files the dumps
        // list the same namespaces.
        let namespaces = |lines: &[String]| -> Vec<String> {
            lines.iter().filter(|l| l.starts_with("SEM ns_full ")).cloned().collect()
        };
        assert_eq!(namespaces(&by_the_caller), namespaces(&by_the_resolver), "{main}");
        assert!(!namespaces(&by_the_resolver).is_empty());
    }
}
