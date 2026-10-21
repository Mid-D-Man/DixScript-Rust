// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/arena-ast.md, section "tests/arena_merge.rs"
// ============================================================================
//! Tests for `Arena::append` and the `shifted` method of the `Doc*` sections.
//!
//! The sections of a file are parsed in parallel, each into an arena of its own, and have to end
//! up in one arena. The check is that nothing is lost or mixed up on the way: sections built in
//! different arenas, appended and shifted into one, must export to exactly the owned sections
//! they were built from.
//!
//! Two files, A and B, each fill every one of the arena's eleven stores (values, expressions,
//! statements, object properties, switch cases, unknown elements, and the value, expression,
//! statement and name lists, plus the text). They are merged in both directions, so each arena is
//! the one appended to once and the one appended once. Because the destination already holds
//! nodes in every store, every offset is nonzero, and a store that is not shifted breaks the
//! export. The corpus test does the same for every file in `mdix_files/` that compiles, with each
//! section in an arena of its own.

use dixscript::Compiler::AST::arena::{
    Arena, DocData, DocQuickFuncs, DocRawBlock, DocSchema, DocSecurity, Document,
};
use dixscript::Compiler::AST::data_types::CompatibilityMode;
use dixscript::Compiler::AST::{DataEntry, DixScript, Position, Value};
use dixscript::Compiler::Core::Config::{ConfigSectionHandler, OperationalSettings};
use dixscript::Compiler::Core::GeneralParser;
use dixscript::Compiler::Core::Tokenizer::{split_config_tokens, Tokenizer};
use dixscript::Compiler::VersionControl::CompatibilityResult;
use dixscript::ErrorManager::ErrorManager;
use dixscript::Runtime::DixLoader;
use std::path::{Path, PathBuf};

/// Parses `source` through the loader's first four stages, which builds the owned AST of every
/// section without analysing it.
fn parse(source: &str) -> DixScript {
    let label = "arena-merge-test";
    let em = ErrorManager::new_isolated_silent();
    let initial = OperationalSettings {
        source_file_path: Some(label.to_string()),
        compatibility_mode: CompatibilityMode::BestEffort,
        ..OperationalSettings::default()
    };
    let tokens = Tokenizer::new_with_error_manager_from_bytes(source.as_bytes(), &initial, em.clone()).tokenize();
    let split = split_config_tokens(tokens.tokens);
    let mut handler = ConfigSectionHandler::new_with_error_manager(None, em.clone());
    let config = handler.process_config_tokens(&split.config_tokens);
    let mut settings = config.operational_settings;
    settings.source_file_path = Some(label.to_string());
    em.update_settings(settings.clone());
    GeneralParser::new_for_lsp(split.rest_tokens, &config.config_section, &settings, em.clone())
        .expect("the parser starts")
        .parse()
        .expect("the parse returns an AST")
}

/// A file with every section filled and every arena store used. `n` makes the two samples
/// differ in every value.
fn sample(n: i32) -> DixScript {
    let source = format!(
        r#"@CONFIG(
  features -> "data,raw,schema,quickfuncs"
)
@QUICKFUNCS(
  ~f<int> => global (a<int> = {n}, b = [1, 2]) {{
    let x = a + {n} * 2
    let y = $"v {{x}}"
    let g = (p, q) => p + q
    let g2 = (p) => {{ let z = p + {n}
      return z }}
    if: x > 1 {{ log: "a" }} elif: x == 0 {{ log: "b" }} else {{ log: "c" }}
    chk: x {{ -> {n} then log: "one{n}" -> miss {{ log: "other{n}" }} }}
    return lib.h(x, [{n}, 2], {{k = x}}).m.n
  }}
  ~g() {{
    return t:(1, [2], {n})
  }}
)
@DATA(
  a = {n}
  o = {{k = [1, {{j = {n}}}], s = "str{n}"}}
  c = f({n}, [1], {{k = 2}}, lib.g(3))
  t.u: x = {n}, y = [1, 2]
  g:: [1, 2], {{a = 1}}
)
@SECURITY(
  encryption -> {{ mode = "password", iterations = {n} }}
  validation -> {{ level = "strict{n}" }}
)
@RAW(
  meta_data -> {{ id = "raw{n}", size = {n} }}
  content -> {{
---tag{n}
BYTES
---tag{n}
  }}
)
@SCHEMA(
  a = Schema.Int(true, 1, {n})
  t.u.x = Schema.Int(false)
)
"#
    );
    let mut ds = parse(&source);
    // A value of unknown type, which is the only thing that uses the arena's unknown store.
    ds.data.as_mut().expect("the sample has data").entries.push(DataEntry::SimpleProperty {
        name: "unknown".to_string(),
        data_type: None,
        value: Value::Unknown {
            element_type: "future".to_string(),
            element_name: format!("element{n}"),
            element_data: "payload".to_string(),
            compatibility_result: CompatibilityResult::compatible_with_warnings(vec!["future element".to_string()]),
            position: Position::new(1, 1),
        },
        position: Position::new(1, 1),
    });
    // A range, which the parser does not produce either, so the `Range` node is covered too.
    ds.data.as_mut().expect("the sample has data").entries.push(DataEntry::SimpleProperty {
        name: "range".to_string(),
        data_type: None,
        value: Value::Range {
            start: Box::new(Value::Integer { value: n, position: Position::new(1, 1) }),
            end: Box::new(Value::Integer { value: n + 10, position: Position::new(1, 2) }),
            position: Position::new(1, 1),
        },
        position: Position::new(1, 1),
    });
    ds
}

/// A section of each kind, built in an arena of its own.
struct Built {
    arena: Arena,
    data: DocData,
    security: DocSecurity,
    raw: Vec<DocRawBlock>,
    schema: DocSchema,
    quick: DocQuickFuncs,
}

fn build(ds: &DixScript) -> Built {
    let mut arena = Arena::new();
    let data = DocData::from_owned(&mut arena, ds.data.as_ref().expect("data"));
    let security = DocSecurity::from_owned(&mut arena, ds.security.as_ref().expect("security"));
    let raw = ds.raw.iter().map(|b| DocRawBlock::from_owned(&mut arena, b)).collect();
    let schema = DocSchema::from_owned(&mut arena, ds.schema.as_ref().expect("schema"));
    let quick = DocQuickFuncs::from_owned(&mut arena, ds.quick_functions.as_ref().expect("quickfuncs"));
    Built { arena, data, security, raw, schema, quick }
}

fn debug(ds: &DixScript) -> String {
    format!("{:#?}", ds)
}

/// Builds a document from three arenas appended in turn to the arena of `base`: data and security
/// come from `first`, the quick functions, schema and raw block from `second`, plus `first`'s raw
/// block. Every section is moved with the offsets of its own arena, and both offsets are nonzero
/// because `base` and then `first` already hold nodes in every store.
fn merge(base: Built, first: Built, second: Built) -> Document {
    let mut arena = base.arena;
    let first_offsets = arena.append(&first.arena);
    let second_offsets = arena.append(&second.arena);
    let mut raw: Vec<DocRawBlock> = first.raw.into_iter().map(|b| b.shifted(first_offsets)).collect();
    raw.extend(second.raw.into_iter().map(|b| b.shifted(second_offsets)));
    Document {
        arena,
        config: None,
        imports: None,
        dlm: None,
        enums: None,
        quick_functions: Some(second.quick.shifted(second_offsets)),
        data: Some(first.data.shifted(first_offsets)),
        security: Some(first.security.shifted(first_offsets)),
        raw,
        schema: Some(second.schema.shifted(second_offsets)),
    }
}

/// The owned file the merged document must export to.
fn expected(first: &DixScript, second: &DixScript) -> DixScript {
    let mut ds = DixScript::new();
    ds.quick_functions = second.quick_functions.clone();
    ds.data = first.data.clone();
    ds.security = first.security.clone();
    ds.raw = first.raw.iter().chain(second.raw.iter()).cloned().collect();
    ds.schema = second.schema.clone();
    ds
}

#[test]
fn the_samples_use_every_section_and_differ() {
    let (a, b) = (sample(1), sample(2));
    for ds in [&a, &b] {
        assert!(ds.data.is_some() && ds.security.is_some() && ds.schema.is_some() && ds.quick_functions.is_some());
        assert_eq!(ds.raw.len(), 1);
    }
    assert_ne!(debug(&a), debug(&b));
}

#[test]
fn appending_shifts_every_store_so_that_merged_sections_export_exactly() {
    let (a, b) = (sample(1), sample(2));
    // Merge in both directions so that each arena is both the destination and the appended one.
    for (first, second) in [(&a, &b), (&b, &a)] {
        let merged = merge(build(&sample(3)), build(first), build(second));
        let exported = merged.export();
        let want = expected(first, second);
        assert_eq!(exported.data, want.data, "data");
        assert_eq!(exported.security, want.security, "security");
        assert_eq!(exported.schema, want.schema, "schema");
        assert_eq!(exported.raw, want.raw, "raw");
        assert_eq!(debug(&exported), debug(&want), "the whole file");
        assert_eq!(exported.to_string(), want.to_string(), "the printed file");
    }
}

#[test]
fn every_store_of_the_destination_holds_nodes_so_every_offset_is_nonzero() {
    // `ArenaOffsets` prints its eleven offsets. A zero would mean the destination store was empty
    // when the other arena was appended, and a shift bug in that store would go unnoticed.
    for (first, second) in [(sample(1), sample(2)), (sample(2), sample(1))] {
        let (mut destination, appended) = (build(&first).arena, build(&second).arena);
        let offsets = destination.append(&appended);
        let printed = format!("{:?}", offsets);
        for store in ["values", "exprs", "stmts", "props", "cases", "unknowns", "value_lists", "expr_lists", "stmt_lists", "str_lists", "text"] {
            assert!(
                !printed.contains(&format!("{store}: 0,")) && !printed.contains(&format!("{store}: 0 }}")),
                "the {store} store was empty in the destination: {printed}"
            );
        }
    }
}

#[test]
fn appending_an_empty_arena_changes_nothing_and_appending_to_an_empty_arena_is_a_copy() {
    let ds = sample(3);
    let built = build(&ds);

    // An empty arena appended to a full one leaves the full one as it was, so a section built in
    // it still exports without any shift.
    let mut full = Arena::new();
    let data = DocData::from_owned(&mut full, ds.data.as_ref().unwrap());
    let before = format!("{:?}", full);
    full.append(&Arena::new());
    assert_eq!(format!("{:?}", full), before);
    assert_eq!(data.export(&full), ds.data.clone().unwrap());

    // A full arena appended to an empty one.
    let mut empty = Arena::new();
    let offsets = empty.append(&built.arena);
    assert_eq!(built.data.shifted(offsets).export(&empty), ds.data.clone().unwrap());
    assert_eq!(built.quick.shifted(offsets).export(&empty), ds.quick_functions.clone().unwrap());
}

#[test]
fn several_arenas_appended_in_turn_keep_every_section_intact() {
    // The order the parser uses: one arena per section, appended one after another.
    let ds = sample(4);
    let mut merged = Arena::new();

    let mut a = Arena::new();
    let data = DocData::from_owned(&mut a, ds.data.as_ref().unwrap());
    let mut b = Arena::new();
    let quick = DocQuickFuncs::from_owned(&mut b, ds.quick_functions.as_ref().unwrap());
    let mut c = Arena::new();
    let schema = DocSchema::from_owned(&mut c, ds.schema.as_ref().unwrap());
    let mut d = Arena::new();
    let security = DocSecurity::from_owned(&mut d, ds.security.as_ref().unwrap());
    let mut e = Arena::new();
    let raw = DocRawBlock::from_owned(&mut e, &ds.raw[0]);

    let data = data.shifted(merged.append(&a));
    let quick = quick.shifted(merged.append(&b));
    let schema = schema.shifted(merged.append(&c));
    let security = security.shifted(merged.append(&d));
    let raw = raw.shifted(merged.append(&e));

    assert_eq!(data.export(&merged), ds.data.clone().unwrap());
    assert_eq!(quick.export(&merged), ds.quick_functions.clone().unwrap());
    assert_eq!(schema.export(&merged), ds.schema.clone().unwrap());
    assert_eq!(security.export(&merged), ds.security.clone().unwrap());
    assert_eq!(raw.export(&merged), ds.raw[0]);
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(read) = std::fs::read_dir(dir) else { return };
    for entry in read.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, out);
        } else if path.extension().map_or(false, |x| x == "mdix") {
            out.push(path);
        }
    }
}

#[test]
fn every_compiled_corpus_file_survives_a_section_by_section_merge() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("mdix_files");
    let mut files = Vec::new();
    walk(&root, &mut files);
    files.sort();
    assert!(files.len() >= 50, "expected the corpus under {}", root.display());

    let (mut compiled, mut sections) = (0usize, 0usize);
    for path in &files {
        let Ok(ds) = DixLoader::new_silent().compile_to_resolved_ast(path.to_str().unwrap()) else { continue };
        compiled += 1;

        // Each section in an arena of its own, as the parallel parsers produce them, then merged
        // in the file's section order.
        let mut merged = Arena::new();
        let mut out = DixScript::new();
        out.config = ds.config.clone();
        out.imports = ds.imports.clone();
        out.dlm = ds.dlm.clone();
        out.enums = ds.enums.clone();

        macro_rules! through_own_arena {
            ($field:expr, $Doc:ident, $name:literal) => {
                if let Some(section) = $field.as_ref() {
                    let mut own = Arena::new();
                    let doc = $Doc::from_owned(&mut own, section);
                    let doc = doc.shifted(merged.append(&own));
                    sections += 1;
                    Some(doc.export(&merged))
                } else {
                    None
                }
            };
        }
        out.quick_functions = through_own_arena!(ds.quick_functions, DocQuickFuncs, "quickfuncs");
        out.data = through_own_arena!(ds.data, DocData, "data");
        out.security = through_own_arena!(ds.security, DocSecurity, "security");
        for block in &ds.raw {
            let mut own = Arena::new();
            let doc = DocRawBlock::from_owned(&mut own, block);
            let doc = doc.shifted(merged.append(&own));
            out.raw.push(doc.export(&merged));
            sections += 1;
        }
        out.schema = through_own_arena!(ds.schema, DocSchema, "schema");

        assert_eq!(debug(&out), debug(&ds), "{}", path.display());
    }
    assert!(compiled >= 60, "only {compiled} corpus files compiled");
    assert!(sections >= 100, "only {sections} sections were merged");
}
