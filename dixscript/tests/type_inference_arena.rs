// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/arena-ast.md, section "tests/type_inference_arena.rs"
// ============================================================================
//! Recorded test for `TypeInferenceVisitor`.
//!
//! `TypeInferenceVisitor` infers the type of a value or an expression from the symbol table and
//! from the types of the local variables it was given. Its arena entry points (`infer_type_from_value_id`
//! and the three others) hold the only implementation; the owned entry points
//! (`infer_type_from_value` and the three others) copy their argument into a scratch arena and call
//! them. The test compiles every file in `mdix_files/` and a set of crafted sources, takes every value
//! and expression node of each document, and checks two things:
//!
//! * the owned entry point and the arena entry point give the same answer on the node, for its type
//!   and for its element type (the wrapper loses nothing in the copy);
//! * the answers are the ones the original owned implementation gave. Each file's answers are folded
//!   into a fingerprint (`CORPUS_RECORDING` and `CRAFTED_RECORDING`), recorded by running the
//!   original code before it was removed.
//!
//! Both are done under three visitor configurations: no local variables, local variables of assorted
//! types, and local variables with element-type hints, so that the branches that depend on them are
//! reached; and with the symbol table the semantic analysis produced for the file, so that functions,
//! data variables, enums and imported namespaces resolve.
//!
//! The nodes come from `Arena::value_ids` and `expr_ids`, so the walk includes nodes a failed
//! sub-expression left behind and cannot miss a nested one. A corpus file that is not in the
//! recording is still checked for the first point. A recorded file that has changed fails the second;
//! `print_recording` (an ignored test) prints the tables again.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use dixscript::Compiler::AST::arena::{Arena, Document, ExprNode, ValueNode};
use dixscript::Compiler::AST::data_types::{CompatibilityMode, ElemType, ErrorHandlingStrategy};
use dixscript::Compiler::AST::{DataType, Expression, Position, Value};
use dixscript::Compiler::AST::Visitors::TypeInferenceVisitor;
use dixscript::Compiler::Core::Config::{ConfigSectionHandler, OperationalSettings};
use dixscript::Compiler::Core::Tokenizer::{split_config_tokens, Tokenizer};
use dixscript::Compiler::Core::{GeneralParser, GeneralSemanticAnalyzer};
use dixscript::Compiler::Utilities::{SymbolTable, VariableInfo};
use dixscript::ErrorManager::ErrorManager;

/// Parses and analyzes one source. Returns the document and the symbol table, or `None` when the
/// parse fails.
fn compile(bytes: &[u8], path: &str) -> Option<(Document, SymbolTable)> {
    let em = ErrorManager::new_isolated_silent();
    let initial = OperationalSettings {
        source_file_path: Some(path.to_string()),
        compatibility_mode: CompatibilityMode::Strict,
        ..OperationalSettings::default()
    };
    let tokens = Tokenizer::new_with_error_manager_from_bytes(bytes, &initial, em.clone()).tokenize();
    let split = split_config_tokens(tokens.tokens);
    let mut handler = ConfigSectionHandler::new_with_error_manager(None, em.clone());
    let config = handler.process_config_tokens(&split.config_tokens);
    let mut settings = config.operational_settings;
    settings.source_file_path = Some(path.to_string());
    // Continue, so that the symbol table is filled in even when a section has errors.
    settings.error_handling_strategy = ErrorHandlingStrategy::Continue;
    em.update_settings(settings.clone());
    let document = GeneralParser::new_for_lsp(split.rest_tokens, &config.config_section, &settings, em.clone())
        .ok()?
        .parse_document()
        .ok()?;
    let ast = document.export();
    let result = GeneralSemanticAnalyzer::new_with_error_manager(&ast, &settings, em)
        .with_document(&document)
        .analyze();
    Some((document, result.symbol_table.unwrap_or_else(SymbolTable::new)))
}

/// The types the synthesized local variables cycle through. They cover plain scalars, untyped and
/// typed collections, and the kinds the method tables care about.
fn local_types() -> Vec<Option<DataType>> {
    vec![
        Some(DataType::Int),
        Some(DataType::TypedArray(ElemType::Int)),
        Some(DataType::TypedArray(ElemType::String)),
        Some(DataType::TypedTuple([Some(ElemType::Int), Some(ElemType::String), None, None, None, None])),
        Some(DataType::TypedTuple([Some(ElemType::Float), Some(ElemType::Float), None, None, None, None])),
        Some(DataType::String),
        Some(DataType::Object),
        Some(DataType::Array),
        Some(DataType::Tuple),
        Some(DataType::Regex),
        Some(DataType::Float),
        Some(DataType::Bool),
        Some(DataType::Enum),
        None,
    ]
}

/// Every identifier-like name in the arena, with a stable type assignment.
fn synthesized_locals(arena: &Arena) -> HashMap<String, Option<DataType>> {
    let mut names: BTreeSet<String> = BTreeSet::new();
    for id in arena.expr_ids() {
        match arena.expr(id) {
            ExprNode::Identifier { name, .. } => {
                names.insert(arena.text(*name).to_string());
            }
            ExprNode::QualifiedIdentifier { parts, .. } => {
                if let Some(first) = arena.str_list(*parts).first() {
                    names.insert(arena.text(*first).to_string());
                }
            }
            _ => {}
        }
    }
    for id in arena.value_ids() {
        if let ValueNode::Identifier { value, .. } = arena.value(id) {
            names.insert(arena.text(*value).to_string());
        }
    }
    let types = local_types();
    names
        .into_iter()
        .enumerate()
        .map(|(i, name)| (name, types[i % types.len()]))
        .collect()
}

fn hints_for(locals: &HashMap<String, Option<DataType>>) -> HashMap<String, DataType> {
    let hint_types = [DataType::Int, DataType::String, DataType::Float, DataType::Bool];
    let mut names: Vec<&String> = locals.keys().collect();
    names.sort();
    names
        .into_iter()
        .enumerate()
        .filter(|(i, _)| i % 2 == 0)
        .map(|(i, name)| (name.clone(), hint_types[i % hint_types.len()]))
        .collect()
}

/// A running 64 bit FNV-1a hash of the answers given for a file, with how many nodes it covers.
struct Fingerprint {
    hash: u64,
    nodes: usize,
}

impl Fingerprint {
    fn new() -> Self {
        Fingerprint { hash: 0xcbf2_9ce4_8422_2325, nodes: 0 }
    }

    fn push(&mut self, text: &str) {
        for byte in text.bytes() {
            self.hash ^= u64::from(byte);
            self.hash = self.hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        self.hash ^= 0xff;
        self.hash = self.hash.wrapping_mul(0x0000_0100_0000_01b3);
        self.nodes += 1;
    }
}

/// What one comparison run saw, to show the test was not vacuous.
#[derive(Default)]
struct Tally {
    nodes: usize,
    some: usize,
    none: usize,
    kinds: BTreeSet<String>,
}

/// Checks the owned and arena entry points against each other on every node of `document`, and
/// returns the fingerprint of the answers.
fn compare(label: &str, document: &Document, table: &SymbolTable, tally: &mut Tally) -> Fingerprint {
    let mut fingerprint = Fingerprint::new();
    let arena = &document.arena;
    let locals = synthesized_locals(arena);
    let hints = hints_for(&locals);
    let configs: [(&str, Option<HashMap<String, Option<DataType>>>, Option<HashMap<String, DataType>>); 3] = [
        ("empty", None, None),
        ("locals", Some(locals.clone()), None),
        ("locals+hints", Some(locals.clone()), Some(hints)),
    ];

    for (config_name, local_types, element_hints) in configs {
        let visitor = TypeInferenceVisitor::new_with_element_hints(table, local_types, element_hints);

        for id in arena.value_ids() {
            let owned = arena.export_value(id);
            let by_owned = visitor.infer_type_from_value(&owned);
            let by_arena = visitor.infer_type_from_value_id(arena, id);
            assert_eq!(by_owned, by_arena, "{label} [{config_name}]: type of value {id:?}: {owned:?}");
            let elem_owned = visitor.infer_element_type_from_value(&owned);
            let elem_arena = visitor.infer_element_type_from_value_id(arena, id);
            assert_eq!(elem_owned, elem_arena, "{label} [{config_name}]: element type of value {id:?}: {owned:?}");

            fingerprint.push(&format!("{config_name}/v{}:{by_arena:?}:{elem_arena:?}", fingerprint.nodes));
            tally.nodes += 1;
            match by_arena {
                Some(t) => {
                    tally.some += 1;
                    tally.kinds.insert(format!("{t:?}"));
                }
                None => tally.none += 1,
            }
            if elem_arena.is_some() {
                tally.kinds.insert("element-of-value".to_string());
            }
        }

        for id in arena.expr_ids() {
            let owned = arena.export_expr(id);
            let by_owned = visitor.infer_type_from_expression(&owned);
            let by_arena = visitor.infer_type_from_expression_id(arena, id);
            assert_eq!(by_owned, by_arena, "{label} [{config_name}]: type of expression {id:?}: {owned:?}");
            let elem_owned = visitor.infer_element_type_from_expression(&owned);
            let elem_arena = visitor.infer_element_type_from_expression_id(arena, id);
            assert_eq!(elem_owned, elem_arena, "{label} [{config_name}]: element type of expression {id:?}: {owned:?}");

            fingerprint.push(&format!("{config_name}/e{}:{by_arena:?}:{elem_arena:?}", fingerprint.nodes));
            tally.nodes += 1;
            match by_arena {
                Some(t) => {
                    tally.some += 1;
                    tally.kinds.insert(format!("{t:?}"));
                }
                None => tally.none += 1,
            }
            if elem_arena.is_some() {
                tally.kinds.insert("element-of-expression".to_string());
            }
        }
    }
    fingerprint
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir).expect("readable corpus").filter_map(|e| e.ok()).collect();
    entries.sort_by_key(|e| e.path());
    for e in entries {
        let p = e.path();
        if p.is_dir() {
            walk(&p, out);
        } else if p.extension().map(|x| x == "mdix").unwrap_or(false) {
            out.push(p);
        }
    }
}

/// (path under `mdix_files`, nodes covered, fingerprint of the answers), recorded from the original
/// owned implementation.
const CORPUS_RECORDING: &[(&str, usize, u64)] = &[
    ("ComparisonFiles/MDIX/AdvancedTest.mdix", 3963, 0x6b422650d0c285ce),
    ("ComparisonFiles/MDIX/BasicTest.mdix", 51, 0x71efa83c5d2e091e),
    ("ComparisonFiles/MDIX/BigBoy.mdix", 2136, 0xd3b8d373412e51e9),
    ("ComparisonFiles/MDIX/IntermediateTest.mdix", 309, 0x21a98e2c0320af55),
    ("ComparisonFiles/MDIX/Unholy.mdix", 15444, 0x53c0df99f9ceecef),
    ("advanced/BuiltinSystem.mdix", 153, 0xf65f2b3e14d0b55f),
    ("advanced/EnumWithCompressionAndOrEnc.mdix", 12, 0x8e3cc83cc71a6065),
    ("advanced/EnumsWithStuffImported.mdix", 6, 0x2a285c4bfdc835d2),
    ("advanced/FullFunctionTest.mdix", 7893, 0x32071c0330749ad7),
    ("advanced/all_datatypes_test.mdix", 1158, 0x28c53a6d9dd7b963),
    ("advanced/array_instance_test.mdix", 1359, 0xe86e05070912b629),
    ("advanced/array_static_test.mdix", 798, 0xe7476c1028626c06),
    ("advanced/data_variable_usage.mdix", 657, 0x80d825aa7a5d9e80),
    ("advanced/datetime_static_test.mdix", 678, 0x7aceeaef7b7a53f3),
    ("advanced/dix_static_test.mdix", 801, 0x28bb65ef404db270),
    ("advanced/enum_static_test.mdix", 708, 0xc3e964f34c1a8f50),
    ("advanced/guid_static_test.mdix", 345, 0xd3f04f7ba92dabe7),
    ("advanced/ip_static_test.mdix", 510, 0xb3e57e48d84a6ce2),
    ("advanced/json_decoder.mdix", 2787, 0x7ad5f1e15e32b6dc),
    ("advanced/math_static_test.mdix", 627, 0xafc73a4e7de1d345),
    ("advanced/number_instance_test.mdix", 525, 0x86b0f30a1e936e86),
    ("advanced/object_builtins_test.mdix", 852, 0xfae8acbe4aa83221),
    ("advanced/random_static_test.mdix", 642, 0x1191500df2ad5ce0),
    ("advanced/string_instance_test.mdix", 600, 0x692dc81f9234da92),
    ("advanced/tuple_instance_test.mdix", 678, 0x4e21eae9faec7b5f),
    ("advanced/universal_instance_test.mdix", 486, 0x0237a12fd557430a),
    ("basic/EnumUsage.mdix", 252, 0x4a9264d1bd691499),
    ("basic/a_test.mdix", 48, 0x2d4a08ce167a69fd),
    ("basic/basic_test.mdix", 177, 0x2ca453447e5591be),
    ("basic/lsp_tester.mdix", 663, 0xfe49341e94162c89),
    ("basic/tuple_instance_test.mdix", 678, 0x4e21eae9faec7b5f),
    ("chemistry_db/core/builders.mdix", 1194, 0x7e303dec2d159033),
    ("chemistry_db/core/enums.mdix", 0, 0xcbf29ce484222325),
    ("chemistry_db/core/physics.mdix", 525, 0x8945e2f91db0ba85),
    ("chemistry_db/core/units.mdix", 2766, 0x481971f2ad8247fb),
    ("chemistry_db/elements_database.mdix", 87975, 0x0d9a464a46c218c2),
    ("intermediate/CompleteFeature.mdix", 300, 0x6e5a669cde3b77f8),
    ("intermediate/ComplexData.mdix", 291, 0x42e0c46d35aadb09),
    ("intermediate/ComplexFull.mdix", 3249, 0x7360c52101629e25),
    ("intermediate/Configuration.mdix", 81, 0x8a7eaf744314f90b),
    ("intermediate/EnumUsage.mdix", 252, 0x4a9264d1bd691499),
    ("intermediate/ErrorHandling.mdix", 42, 0x1d0a994cc1a09c52),
    ("intermediate/FullDataTestNoFunc.mdix", 2256, 0x349f180e3f6675f1),
    ("intermediate/FullDataTestWithFunc.mdix", 10818, 0x39e799007803524a),
    ("intermediate/FunctionScoping.mdix", 237, 0xcf453e806fb55b00),
    ("tests/cli/01_basic.mdix", 33, 0x678c7cd3cf3da372),
    ("tests/cli/02_enums.mdix", 33, 0x014bb7fbc4043fd5),
    ("tests/cli/03_quickfuncs.mdix", 213, 0xc63e46c077248f34),
    ("tests/cli/04_all_types.mdix", 144, 0xfa23ba423b8c96f0),
    ("tests/cli/05_game_config.mdix", 285, 0xb19c7137ac84a5b9),
    ("tests/cli/06_app_config.mdix", 87, 0x8d4063a5d4b019df),
    ("tests/dlm/01_gzip.mdix", 39, 0x517b89af4a5b5e45),
    ("tests/dlm/02_bzip2.mdix", 39, 0x517b89af4a5b5e45),
    ("tests/dlm/03_lzma.mdix", 39, 0x517b89af4a5b5e45),
    ("tests/dlm/04_aes128_password.mdix", 72, 0x368b08cc5c565512),
    ("tests/dlm/05_aes256_password.mdix", 66, 0x1842a483b06c7dbf),
    ("tests/dlm/06_chacha20_password.mdix", 81, 0x62bae6f3a99e2148),
    ("tests/dlm/07_diy_audit.mdix", 39, 0x517b89af4a5b5e45),
    ("tests/dlm/08_enhanced_audit.mdix", 42, 0x37e092f3312e512e),
    ("tests/dlm/09_gzip_aes256.mdix", 81, 0x62bae6f3a99e2148),
    ("tests/dlm/10_full_pipeline.mdix", 81, 0x62bae6f3a99e2148),
    ("tests/dlm/11_enum_only_gzip.mdix", 18, 0xdb7a58f6824130d0),
    ("tests/dlm/12_enum_only_aes256.mdix", 51, 0x0276a52726157dce),
    ("tests/dlm/13_enum_only_gzip_aes256.mdix", 51, 0x0276a52726157dce),
    ("tests/dlm/security_01_keyfile_aes256.mdix", 273, 0x865fe9ab0a465787),
    ("tests/dlm/security_02_password_aes256.mdix", 246, 0xfac06979256c6e0a),
    ("tests/dlm/security_03_keyfile_gzip_aes256.mdix", 333, 0x5070fb4003ef96c3),
    ("tests/dlm/security_05_password_full_pipeline.mdix", 666, 0xd0b480ddbc49c860),
    ("tests/dlm/serialize_target.mdix", 60, 0xac7a6fac65145f83),
    ("tests/imports/01_basic_import.mdix", 36, 0xa964e5e0d7a1167e),
    ("tests/imports/02_alias_funcs_in_quickfuncs.mdix", 144, 0xfb6d99566f172c31),
    ("tests/imports/03_alias_funcs_in_data.mdix", 192, 0xaa71f82bff794d86),
    ("tests/imports/04_funcs_in_funcs.mdix", 183, 0xcfa9058fb31a5d5d),
    ("tests/imports/05_enum_in_funcs.mdix", 246, 0xe2eedceba4b09cbf),
    ("tests/imports/06_cloud_basic.mdix", 120, 0x891fa0c25863b811),
    ("tests/imports/07_cloud_verify_invalid.mdix", 30, 0x943d8a3b355e62b9),
    ("tests/imports/09_cloud_transitive.mdix", 171, 0x6b7116c565d87356),
    ("tests/imports/10_cloud_cyclic.mdix", 9, 0x12cbf800e843bef5),
    ("tests/imports/11_enum_only_import_no_funcs.mdix", 30, 0x1ad9c467e31b4856),
    ("tests/imports/base_types.mdix", 150, 0xcc3019eda0b2ffca),
    ("tests/imports/circular_a.mdix", 3, 0xffddf111accdf15e),
    ("tests/imports/circular_b.mdix", 3, 0xffddf111accdf15e),
    ("tests/imports/enum_only_types.mdix", 0, 0xcbf29ce484222325),
    ("tests/imports/game_helpers.mdix", 174, 0xcff2aa979179a167),
];

/// The same for the crafted sources below.
const CRAFTED_RECORDING: &[(&str, usize, u64)] = &[
    ("static_calls", 189, 0x58c0c37c2761bdd2),
    ("instance_methods", 204, 0xee5cc824434f42c5),
    ("operators", 189, 0x2a8b6f7848f13f20),
    ("data_values_and_access", 126, 0xdb74a622fef167d2),
];

fn corpus_files() -> (PathBuf, Vec<PathBuf>) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("mdix_files");
    let mut files = Vec::new();
    walk(&root, &mut files);
    assert!(files.len() >= 50, "expected the corpus under {}, found {}", root.display(), files.len());
    (root, files)
}

fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root).expect("under the corpus").to_string_lossy().replace('\\', "/")
}

#[test]
fn the_entry_points_agree_and_give_the_recorded_answers_on_every_node_of_the_corpus() {
    let (root, files) = corpus_files();

    let mut tally = Tally::default();
    let mut compiled = 0;
    let mut matched = 0;
    for path in &files {
        let bytes = std::fs::read(path).expect("readable file");
        let Some((document, table)) = compile(&bytes, &path.to_string_lossy()) else { continue };
        let fingerprint = compare(&path.display().to_string(), &document, &table, &mut tally);
        compiled += 1;
        let name = relative(&root, path);
        if let Some((_, nodes, hash)) = CORPUS_RECORDING.iter().find(|(n, _, _)| *n == name) {
            assert_eq!(
                (fingerprint.nodes, fingerprint.hash),
                (*nodes, *hash),
                "{name}: the answers differ from the recording (if the file changed on purpose, run the ignored `print_recording` test)"
            );
            matched += 1;
        }
    }

    println!(
        "{compiled} files ({matched} recorded), {} comparisons ({} with a type, {} without), kinds seen: {:?}",
        tally.nodes, tally.some, tally.none, tally.kinds
    );
    assert!(compiled >= 60, "only {compiled} corpus files compiled");
    assert!(matched >= 60, "only {matched} corpus files are in the recording");
    assert!(tally.nodes >= 20_000, "only {} comparisons", tally.nodes);
    assert!(tally.some > 5_000 && tally.none > 1_000, "the comparison was lopsided: {} typed, {} untyped", tally.some, tally.none);
    // The comparison must have seen results of several different kinds, not only integers.
    for kind in ["Int", "String", "Bool", "Double", "Enum", "Object", "Array"] {
        assert!(tally.kinds.contains(kind), "no node was inferred as {kind}; saw {:?}", tally.kinds);
    }
    assert!(
        tally.kinds.iter().any(|k| k.starts_with("TypedArray")) && tally.kinds.iter().any(|k| k.starts_with("TypedTuple")),
        "no typed collection was inferred; saw {:?}",
        tally.kinds
    );
}

/// Sources that reach branches the corpus is unlikely to: builtin static calls whose result
/// depends on their arguments, instance methods on typed collections, conditionals, casts,
/// property and index access.
const CRAFTED: &[(&str, &str)] = &[
    (
        "static_calls",
        "@QUICKFUNCS(\n  ~f() {\n    let a = Array.of(1, 2, 3)\n    let b = Array.of(1, \"x\")\n    let c = Array.range(1, 5)\n    let d = Array.fill(0, 3)\n    let e = Array.concat(a, c)\n    let g = Array.reverse(a)\n    let h = Array.repeat(a, 2)\n    let i = Random.choice(a)\n    let j = Random.choices(a, 2)\n    let k = Array.empty()\n    let l = Array.fromString(\"a,b\", \",\")\n    let m = Array.flatten(a)\n    let n = Math.sqrt(4)\n    return a\n  }\n)\n@DATA(\n  x = 1\n)\n",
    ),
    (
        "instance_methods",
        "@QUICKFUNCS(\n  ~f() {\n    let a = [1, 2, 3]\n    let s = \"a b\"\n    let t = t:(1, \"x\", true)\n    let r = r:(\"a+\")\n    let o = {k = 1}\n    let a1 = a.first()\n    let a2 = a.reverse()\n    let a3 = a.flatten()\n    let s1 = s.split(\" \")\n    let s2 = s.upper()\n    let t1 = t.first()\n    let t2 = t.get(1)\n    let t3 = t.toArray()\n    let t4 = t.reverse()\n    let r1 = r.match(\"aa\")\n    let r2 = r.matchAll(\"aa\")\n    let o1 = o.keys()\n    let o2 = o.values()\n    let o3 = o.count()\n    return a1\n  }\n)\n@DATA(\n  x = 1\n)\n",
    ),
    (
        "operators",
        "@QUICKFUNCS(\n  ~f(n) {\n    let a = 1 + 2\n    let b = 1 + 2.5\n    let c = 1L + 2\n    let d = \"a\" + 1\n    let e = 1.5f + 2\n    let g = !true\n    let h = 1 < 2\n    let i = true && false\n    let j = 5 & 3\n    let k = -n\n    let l = true ? 1 : 2\n    let m = (1 + 2) * 3\n    return a\n  }\n)\n@DATA(\n  x = 1\n)\n",
    ),
    (
        "data_values_and_access",
        "@ENUMS(\n  LogLevel { DEBUG = 0, INFO = 1 }\n)\n@DATA(\n  level<enum> = LogLevel.INFO\n  nums = [1, 2, 3]\n  mixed = [1, \"a\"]\n  pair = t:(1, \"x\")\n  triple = t:(1.5, 2.5, 3.5)\n  blob = b:(\"SGVsbG8=\")\n  re = r:(\"a+\")\n  obj = {a = 1, b = {c = 2}}\n  empty = []\n  n = null\n  nested = [[1, 2], [3]]\n)\n@QUICKFUNCS(\n  ~g() {\n    let x = nums[0]\n    let y = obj.b.c\n    let z = DATA.nums\n    let p = pair.first()\n    return x\n  }\n)\n",
    ),
];

#[test]
fn the_entry_points_agree_and_give_the_recorded_answers_on_crafted_sources() {
    let mut tally = Tally::default();
    let mut compiled = 0;
    for (name, text) in CRAFTED {
        let Some((document, table)) = compile(text.as_bytes(), name) else { continue };
        let fingerprint = compare(name, &document, &table, &mut tally);
        let (_, nodes, hash) = CRAFTED_RECORDING
            .iter()
            .find(|(n, _, _)| n == name)
            .unwrap_or_else(|| panic!("no recording for {name}"));
        assert_eq!((fingerprint.nodes, fingerprint.hash), (*nodes, *hash), "{name}: the answers differ from the recording");
        compiled += 1;
    }
    println!("{compiled} crafted sources, {} comparisons, kinds seen: {:?}", tally.nodes, tally.kinds);
    assert_eq!(compiled, CRAFTED.len(), "a crafted source no longer compiles");
    assert!(tally.nodes >= 300, "only {} comparisons", tally.nodes);
}

/// Prints `CORPUS_RECORDING` and `CRAFTED_RECORDING` for the current code. Run it with
/// `cargo test --test type_inference_arena print_recording -- --ignored --nocapture` after a change
/// to a corpus file or to the sources above, and paste the output over the two tables.
#[test]
#[ignore]
fn print_recording() {
    let (root, files) = corpus_files();
    println!("CORPUS_RECORDING");
    for path in &files {
        let bytes = std::fs::read(path).expect("readable file");
        let Some((document, table)) = compile(&bytes, &path.to_string_lossy()) else { continue };
        let f = compare("recording", &document, &table, &mut Tally::default());
        println!("    (\"{}\", {}, 0x{:016x}),", relative(&root, path), f.nodes, f.hash);
    }
    println!("CRAFTED_RECORDING");
    for (name, text) in CRAFTED {
        let Some((document, table)) = compile(text.as_bytes(), name) else { continue };
        let f = compare(name, &document, &table, &mut Tally::default());
        println!("    (\"{name}\", {}, 0x{:016x}),", f.nodes, f.hash);
    }
}

// ==================== nodes built by hand ====================
//
// The expected answers below were recorded from the original owned implementation.

fn pos() -> Position {
    Position::new(1, 1)
}

fn int(n: i32) -> Value {
    Value::Integer { value: n, position: pos() }
}

fn text(t: &str) -> Value {
    Value::String { value: t.to_string(), position: pos() }
}

fn ident(name: &str) -> Expression {
    Expression::Identifier { name: name.to_string(), position: pos() }
}

fn ctor(prefix: &str, arguments: Vec<Value>) -> Value {
    Value::PrefixedConstructor { prefix: prefix.to_string(), arguments, position: pos() }
}

fn variable(name: &str, t: DataType) -> VariableInfo {
    VariableInfo {
        name: name.to_string(),
        declared_type: Some(t),
        inferred_type: Some(t),
        is_inferred: false,
        scope: "global".to_string(),
        line: 1,
        column: 1,
    }
}

/// Checks the owned and arena entry points against each other on one owned value, and returns what
/// they agreed on.
fn same_value(visitor: &TypeInferenceVisitor, label: &str, value: &Value) -> (Option<DataType>, Option<DataType>) {
    let mut arena = Arena::new();
    let id = arena.import_value(value);
    let owned = (visitor.infer_type_from_value(value), visitor.infer_element_type_from_value(value));
    let arena_side = (visitor.infer_type_from_value_id(&arena, id), visitor.infer_element_type_from_value_id(&arena, id));
    assert_eq!(owned, arena_side, "{label}: {value:?}");
    owned
}

/// Checks the owned and arena entry points against each other on one owned expression, and returns
/// what they agreed on.
fn same_expression(visitor: &TypeInferenceVisitor, label: &str, expr: &Expression) -> (Option<DataType>, Option<DataType>) {
    let mut arena = Arena::new();
    let id = arena.import_expr(expr);
    let owned = (visitor.infer_type_from_expression(expr), visitor.infer_element_type_from_expression(expr));
    let arena_side = (
        visitor.infer_type_from_expression_id(&arena, id),
        visitor.infer_element_type_from_expression_id(&arena, id),
    );
    assert_eq!(owned, arena_side, "{label}: {expr:?}");
    owned
}

#[test]
fn constructor_prefixes_are_matched_ignoring_case() {
    let table = SymbolTable::new();
    let visitor = TypeInferenceVisitor::new(&table, None);

    // `T:(1, 2)` and `t:(1, 2)` are the same tuple, and the element type of either is its first
    // argument's type.
    for prefix in ["t", "T"] {
        let (t, element) = same_value(&visitor, "tuple", &ctor(prefix, vec![int(1), int(2)]));
        assert!(matches!(t, Some(DataType::TypedTuple(_))), "{prefix}: {t:?}");
        assert_eq!(element, Some(DataType::Int), "{prefix}");
    }
    for (prefix, expected) in [("b", DataType::Blob), ("B", DataType::Blob), ("r", DataType::Regex), ("R", DataType::Regex)] {
        assert_eq!(same_value(&visitor, "constructor", &ctor(prefix, vec![text("x")])).0, Some(expected), "{prefix}");
    }
    // An unknown prefix has no type.
    assert_eq!(same_value(&visitor, "unknown prefix", &ctor("z", vec![int(1)])).0, None);
    // A tuple of more than six arguments types only the first six.
    let long = ctor("t", (1..=8).map(int).collect());
    assert!(matches!(same_value(&visitor, "long tuple", &long).0, Some(DataType::TypedTuple(_))));
    // A tuple whose arguments have no element type is an untyped tuple.
    let untyped = ctor("t", vec![Value::Null { position: pos() }]);
    assert_eq!(same_value(&visitor, "untyped tuple", &untyped).0, Some(DataType::Tuple));
}

#[test]
fn an_object_access_path_is_looked_up_with_dots() {
    let mut table = SymbolTable::new();
    table.add_data_variable("server.port".to_string(), variable("server.port", DataType::Int));
    table.add_data_variable("DATA.config.name".to_string(), variable("DATA.config.name", DataType::String));
    let visitor = TypeInferenceVisitor::new(&table, None);

    let access = |path: &[&str]| Expression::ObjectAccess { path: path.iter().map(|s| s.to_string()).collect(), position: pos() };
    assert_eq!(same_expression(&visitor, "plain path", &access(&["server", "port"])).0, Some(DataType::Int));
    assert_eq!(same_expression(&visitor, "DATA prefix", &access(&["config", "name"])).0, Some(DataType::String));
    assert_eq!(same_expression(&visitor, "unknown path", &access(&["server", "nope"])).0, None);
}

#[test]
fn an_index_access_takes_its_type_from_the_collection() {
    let table = SymbolTable::new();
    let index = |object: Expression| Expression::IndexAccess {
        object: Box::new(object),
        index: Box::new(Expression::Value { value: int(0), position: pos() }),
        position: pos(),
    };

    let tuple = DataType::TypedTuple([Some(ElemType::Int), Some(ElemType::String), None, None, None, None]);
    let cases: [(&str, DataType, Option<DataType>); 7] = [
        ("typed array", DataType::TypedArray(ElemType::Float), Some(DataType::Float)),
        ("typed tuple", tuple, Some(DataType::Int)),
        ("string", DataType::String, Some(DataType::String)),
        ("plain array", DataType::Array, None),
        ("plain tuple", DataType::Tuple, None),
        ("object", DataType::Object, None),
        ("integer", DataType::Int, None),
    ];
    for (label, local, expected) in cases {
        let mut locals = HashMap::new();
        locals.insert("x".to_string(), Some(local));
        let visitor = TypeInferenceVisitor::new(&table, Some(locals));
        assert_eq!(same_expression(&visitor, label, &index(ident("x"))).0, expected, "{label}");
    }

    // With an element-type hint, a plain array has an element type to give.
    let mut locals = HashMap::new();
    locals.insert("x".to_string(), Some(DataType::Array));
    let mut hints = HashMap::new();
    hints.insert("x".to_string(), DataType::Bool);
    let visitor = TypeInferenceVisitor::new_with_element_hints(&table, Some(locals), Some(hints));
    assert_eq!(same_expression(&visitor, "hinted array", &index(ident("x"))).0, Some(DataType::Bool));
}

#[test]
fn collections_and_conditionals_agree_on_hand_built_nodes() {
    let table = SymbolTable::new();
    let visitor = TypeInferenceVisitor::new(&table, None);
    let array = |values: Vec<Value>| Value::Array { values, position: pos() };

    // Uniform, mixed, empty and nested arrays, with and without a typeless item: the type, and the
    // element type, recorded from the original implementation.
    for (label, value, expected) in [
        ("uniform ints", array(vec![int(1), int(2)]), "(Some(TypedArray(Int)), Some(Int))"),
        ("mixed", array(vec![int(1), text("a")]), "(Some(Array), None)"),
        ("empty", array(vec![]), "(Some(Array), None)"),
        ("nested", array(vec![array(vec![int(1)]), array(vec![int(2)])]), "(Some(TypedArray(Array)), Some(TypedArray(Int)))"),
        ("typeless item", array(vec![int(1), Value::Null { position: pos() }, int(3)]), "(Some(Array), Some(Int))"),
    ] {
        assert_eq!(format!("{:?}", same_value(&visitor, label, &value)), expected, "{label}");
    }

    // A conditional takes the first branch that has a type.
    let branch = |v: Value| Expression::Value { value: v, position: pos() };
    let conditional = |a: Value, b: Value| Expression::Conditional {
        condition: Box::new(branch(Value::Boolean { value: true, position: pos() })),
        true_value: Box::new(branch(a)),
        false_value: Box::new(branch(b)),
        position: pos(),
    };
    assert_eq!(same_expression(&visitor, "both typed", &conditional(int(1), text("a"))).0, Some(DataType::Int));
    assert_eq!(same_expression(&visitor, "first untyped", &conditional(Value::Null { position: pos() }, text("a"))).0, Some(DataType::String));
    assert_eq!(same_expression(&visitor, "both untyped", &conditional(Value::Null { position: pos() }, Value::Null { position: pos() })).0, None);
}

fn qualified_call(parts: &[&str]) -> Expression {
    Expression::QualifiedIdentifier {
        parts: parts.iter().map(|s| s.to_string()).collect(),
        arguments: Some(vec![]),
        position: pos(),
    }
}

#[test]
fn a_positional_tuple_method_gives_the_type_of_its_slot() {
    let table = SymbolTable::new();
    let slots = [ElemType::Int, ElemType::String, ElemType::Float, ElemType::Bool, ElemType::Long, ElemType::Double];
    let typed = DataType::TypedTuple(slots.map(Some));
    let gappy = DataType::TypedTuple([Some(ElemType::Int), None, Some(ElemType::Float), None, None, None]);

    let mut locals = HashMap::new();
    locals.insert("t".to_string(), Some(typed));
    locals.insert("g".to_string(), Some(gappy));
    locals.insert("p".to_string(), Some(DataType::Tuple));
    let visitor = TypeInferenceVisitor::new(&table, Some(locals));

    for (method, slot) in [("first", 0), ("second", 1), ("third", 2), ("fourth", 3), ("fifth", 4), ("sixth", 5)] {
        let got = same_expression(&visitor, method, &qualified_call(&["t", method])).0;
        assert_eq!(got, Some(slots[slot].to_data_type()), "t.{method}()");
    }
    // A slot with no annotation, and a tuple with none at all, have no type.
    assert_eq!(same_expression(&visitor, "gap", &qualified_call(&["g", "second"])).0, None);
    assert_eq!(same_expression(&visitor, "gap", &qualified_call(&["g", "third"])).0, Some(DataType::Float));
    assert_eq!(same_expression(&visitor, "plain", &qualified_call(&["p", "first"])).0, None);
    // `get` takes the first annotated slot, and an unknown method has no type.
    assert_eq!(same_expression(&visitor, "get", &qualified_call(&["t", "get"])).0, Some(DataType::Int));
    assert_eq!(same_expression(&visitor, "unknown", &qualified_call(&["t", "seventh"])).0, None);
}

#[test]
fn an_element_hint_types_the_elements_of_a_plain_array_called_through_a_qualified_name() {
    let table = SymbolTable::new();
    let mut locals = HashMap::new();
    locals.insert("xs".to_string(), Some(DataType::Array));
    locals.insert("ys".to_string(), Some(DataType::TypedArray(ElemType::String)));
    let mut hints = HashMap::new();
    hints.insert("xs".to_string(), DataType::Int);
    hints.insert("ys".to_string(), DataType::Bool);

    let hinted = TypeInferenceVisitor::new_with_element_hints(&table, Some(locals.clone()), Some(hints));
    let unhinted = TypeInferenceVisitor::new(&table, Some(locals));
    for method in ["first", "last", "get"] {
        // A plain array takes the hint; a typed array keeps its own element type.
        assert_eq!(same_expression(&hinted, method, &qualified_call(&["xs", method])).0, Some(DataType::Int), "xs.{method}() hinted");
        assert_eq!(same_expression(&unhinted, method, &qualified_call(&["xs", method])).0, None, "xs.{method}() unhinted");
        assert_eq!(same_expression(&hinted, method, &qualified_call(&["ys", method])).0, Some(DataType::String), "ys.{method}()");
    }
}
