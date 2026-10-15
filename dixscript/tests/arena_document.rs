// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/arena-ast.md, section "tests/arena_document.rs"
// ============================================================================
//! Proves the arena-resident `Document` holds a whole `DixScript` exactly:
//! `Document::from_owned(&ds).export() == ds`, and `Document`'s own `==` agrees with the owned
//! `DixScript` `==`.
//!
//! A hand-built file fills every value-holding section (all four `@DATA` entry kinds, `@RAW`
//! with and without content, `@SECURITY`, `@SCHEMA`, `@QUICKFUNCS` with and without optional
//! parts). Then every file in `mdix_files/` that compiles is run through the real compiler and
//! round-tripped whole. Equality is checked by changing exactly one field at a time.

use dixscript::Compiler::AST::arena::Document;
use dixscript::Compiler::AST::{
    DataEntry, DataSection, DataType, DixScript, Expression, Position, PropertyAssignment,
    QuickFuncParam, QuickFuncStatement, QuickFuncsSection, QuickFunction, RawBlock, RawContent,
    RawField, SchemaBlock, SchemaField, SecurityEntry, SecurityField, SecuritySection, TablePath,
    Value,
};
use dixscript::Runtime::DixLoader;
use std::path::{Path, PathBuf};

fn pos(n: usize) -> Position {
    Position::new(n, n + 1)
}

fn int(v: i32, n: usize) -> Value {
    Value::Integer { value: v, position: pos(n) }
}

fn text(t: &str, n: usize) -> Value {
    Value::String { value: t.to_string(), position: pos(n) }
}

fn ident(t: &str, n: usize) -> Expression {
    Expression::Identifier { name: t.to_string(), position: pos(n) }
}

fn path(parts: &[&str]) -> TablePath {
    TablePath::new(parts.iter().map(|p| p.to_string()).collect())
}

/// A file with every value-holding section filled, and the optional parts present in some
/// places and absent in others.
fn base() -> DixScript {
    let mut ds = DixScript::new();

    ds.data = Some(DataSection::new(
        vec![
            DataEntry::SimpleProperty { name: "a".into(), data_type: Some(DataType::Int), value: int(1, 1), position: pos(1) },
            DataEntry::TableProperty {
                path: path(&["t", "u"]),
                properties: vec![
                    PropertyAssignment::new("x".into(), Some(DataType::Long), int(2, 2), pos(2)),
                    PropertyAssignment::new("y".into(), None, text("s", 2), pos(3)),
                ],
                position: pos(2),
            },
            DataEntry::GroupArray { path: path(&["g"]), items: vec![int(3, 3), int(4, 3)], position: pos(3) },
            DataEntry::ObjectProperty {
                name: "o".into(),
                data_type: None,
                object: Box::new(Value::Object { properties: vec![], position: pos(4) }),
                position: pos(4),
            },
        ],
        pos(1),
    ));

    ds.raw = vec![
        RawBlock::new(
            vec![RawField::new("id".into(), text("blob", 5), pos(5))],
            vec![RawField::new("n".into(), int(7, 5), pos(5)), RawField::new("m".into(), int(8, 5), pos(5))],
            Some(RawContent::new("tag".into(), 3, 9, pos(5))),
            pos(5),
        ),
        RawBlock::new(vec![], vec![], None, pos(6)),
    ];

    ds.security = Some(SecuritySection::new(
        vec![SecurityEntry::new(
            "k".into(),
            vec![SecurityField::new("f".into(), text("v", 7), pos(7)), SecurityField::new("g".into(), int(9, 7), pos(7))],
            pos(7),
        )],
        pos(7),
    ));

    ds.schema = Some(SchemaBlock::new(
        vec![SchemaField::new(path(&["t", "x"]), "range".into(), vec![int(1, 8), int(5, 8)], pos(8))],
        pos(8),
    ));

    ds.quick_functions = Some(QuickFuncsSection::new(
        vec![
            QuickFunction::new(
                "f".into(),
                Some(DataType::Int),
                Some(vec!["main".into(), "aux".into()]),
                vec![QuickFuncParam::new("p".into(), Some(DataType::Int), Some(ident("d", 9)), pos(9))],
                vec![QuickFuncStatement::Return { value: ident("p", 9), position: pos(9) }],
                pos(9),
            ),
            QuickFunction::new(
                "g".into(),
                None,
                None,
                vec![QuickFuncParam::new("q".into(), None, None, pos(10))],
                vec![],
                pos(10),
            ),
        ],
        pos(9),
    ));

    ds
}

#[test]
fn a_hand_built_file_round_trips_exactly() {
    let ds = base();
    let doc = Document::from_owned(&ds);
    assert_eq!(doc.export(), ds);
}

#[test]
fn an_empty_file_and_empty_sections_round_trip_and_stay_different_from_missing_ones() {
    let empty = DixScript::new();
    assert_eq!(Document::from_owned(&empty).export(), empty);

    let mut with_empty_sections = DixScript::new();
    with_empty_sections.data = Some(DataSection::new(vec![], pos(1)));
    with_empty_sections.security = Some(SecuritySection::new(vec![], pos(1)));
    with_empty_sections.schema = Some(SchemaBlock::new(vec![], pos(1)));
    with_empty_sections.quick_functions = Some(QuickFuncsSection::new(vec![], pos(1)));
    assert_eq!(Document::from_owned(&with_empty_sections).export(), with_empty_sections);
    assert!(Document::from_owned(&with_empty_sections) != Document::from_owned(&empty));
}

#[test]
fn a_document_is_an_owned_shareable_value_and_its_clone_is_equal() {
    fn assert_owned_and_shareable<T: Send + Sync + 'static + Clone>() {}
    assert_owned_and_shareable::<Document>();

    let doc = Document::from_owned(&base());
    let copy = doc.clone();
    assert!(doc == copy);
    assert_eq!(copy.export(), base());
}

#[test]
fn two_documents_of_the_same_file_are_equal_even_when_their_arenas_differ() {
    // The second copy is imported after unrelated content, so every id differs.
    let first = Document::from_owned(&base());
    let mut other = base();
    other.data.as_mut().unwrap().entries.insert(
        0,
        DataEntry::SimpleProperty { name: "pad".into(), data_type: None, value: int(0, 1), position: pos(1) },
    );
    let padded = Document::from_owned(&other);
    other.data.as_mut().unwrap().entries.remove(0);
    let second = Document::from_owned(&other);
    assert!(first == second);
    assert!(first != padded);
}

/// Every single-field change to `base()`. Each must make the owned files unequal and the
/// documents unequal.
fn one_field_apart() -> Vec<(&'static str, DixScript)> {
    let base = base();
    let mut out: Vec<(&'static str, DixScript)> = Vec::new();
    macro_rules! tweak {
        ($label:expr, |$ds:ident| $body:block) => {{
            let mut $ds = base.clone();
            $body
            out.push(($label, $ds));
        }};
    }

    // @DATA
    tweak!("data: section position", |ds| { ds.data.as_mut().unwrap().position = pos(99); });
    tweak!("data: no section", |ds| { ds.data = None; });
    tweak!("data: one entry fewer", |ds| { ds.data.as_mut().unwrap().entries.pop(); });
    tweak!("data simple: name", |ds| {
        if let DataEntry::SimpleProperty { name, .. } = &mut ds.data.as_mut().unwrap().entries[0] { name.push('x'); }
    });
    tweak!("data simple: data type", |ds| {
        if let DataEntry::SimpleProperty { data_type, .. } = &mut ds.data.as_mut().unwrap().entries[0] { *data_type = None; }
    });
    tweak!("data simple: value", |ds| {
        if let DataEntry::SimpleProperty { value, .. } = &mut ds.data.as_mut().unwrap().entries[0] { *value = int(100, 1); }
    });
    tweak!("data simple: position", |ds| {
        if let DataEntry::SimpleProperty { position, .. } = &mut ds.data.as_mut().unwrap().entries[0] { *position = pos(99); }
    });
    tweak!("data table: path", |ds| {
        if let DataEntry::TableProperty { path, .. } = &mut ds.data.as_mut().unwrap().entries[1] { path.segments.push("z".into()); }
    });
    tweak!("data table: property name", |ds| {
        if let DataEntry::TableProperty { properties, .. } = &mut ds.data.as_mut().unwrap().entries[1] { properties[0].name.push('x'); }
    });
    tweak!("data table: property data type", |ds| {
        if let DataEntry::TableProperty { properties, .. } = &mut ds.data.as_mut().unwrap().entries[1] { properties[0].data_type = None; }
    });
    tweak!("data table: property value", |ds| {
        if let DataEntry::TableProperty { properties, .. } = &mut ds.data.as_mut().unwrap().entries[1] { properties[1].value = text("other", 2); }
    });
    tweak!("data table: property position", |ds| {
        if let DataEntry::TableProperty { properties, .. } = &mut ds.data.as_mut().unwrap().entries[1] { properties[0].position = pos(99); }
    });
    tweak!("data table: one property fewer", |ds| {
        if let DataEntry::TableProperty { properties, .. } = &mut ds.data.as_mut().unwrap().entries[1] { properties.pop(); }
    });
    tweak!("data table: position", |ds| {
        if let DataEntry::TableProperty { position, .. } = &mut ds.data.as_mut().unwrap().entries[1] { *position = pos(99); }
    });
    tweak!("data group: path", |ds| {
        if let DataEntry::GroupArray { path, .. } = &mut ds.data.as_mut().unwrap().entries[2] { path.segments[0].push('x'); }
    });
    tweak!("data group: item value", |ds| {
        if let DataEntry::GroupArray { items, .. } = &mut ds.data.as_mut().unwrap().entries[2] { items[1] = int(100, 3); }
    });
    tweak!("data group: one item fewer", |ds| {
        if let DataEntry::GroupArray { items, .. } = &mut ds.data.as_mut().unwrap().entries[2] { items.pop(); }
    });
    tweak!("data group: position", |ds| {
        if let DataEntry::GroupArray { position, .. } = &mut ds.data.as_mut().unwrap().entries[2] { *position = pos(99); }
    });
    tweak!("data object: name", |ds| {
        if let DataEntry::ObjectProperty { name, .. } = &mut ds.data.as_mut().unwrap().entries[3] { name.push('x'); }
    });
    tweak!("data object: data type", |ds| {
        if let DataEntry::ObjectProperty { data_type, .. } = &mut ds.data.as_mut().unwrap().entries[3] { *data_type = Some(DataType::Int); }
    });
    tweak!("data object: object", |ds| {
        if let DataEntry::ObjectProperty { object, .. } = &mut ds.data.as_mut().unwrap().entries[3] { **object = Value::Null { position: pos(4) }; }
    });
    tweak!("data object: position", |ds| {
        if let DataEntry::ObjectProperty { position, .. } = &mut ds.data.as_mut().unwrap().entries[3] { *position = pos(99); }
    });
    tweak!("data: entry kind", |ds| {
        // A simple property in place of a group array with the same name-less shape.
        ds.data.as_mut().unwrap().entries[2] =
            DataEntry::SimpleProperty { name: "g".into(), data_type: None, value: int(3, 3), position: pos(3) };
    });

    // @RAW
    tweak!("raw: one block fewer", |ds| { ds.raw.pop(); });
    tweak!("raw: meta key", |ds| { ds.raw[0].meta_data[0].key.push('x'); });
    tweak!("raw: meta value", |ds| { ds.raw[0].meta_data[0].value = text("other", 5); });
    tweak!("raw: meta position", |ds| { ds.raw[0].meta_data[0].position = pos(99); });
    tweak!("raw: using key", |ds| { ds.raw[0].using[1].key.push('x'); });
    tweak!("raw: using value", |ds| { ds.raw[0].using[1].value = int(100, 5); });
    tweak!("raw: one using field fewer", |ds| { ds.raw[0].using.pop(); });
    tweak!("raw: content tag", |ds| { ds.raw[0].content.as_mut().unwrap().tag.push('x'); });
    tweak!("raw: content end", |ds| { ds.raw[0].content.as_mut().unwrap().end += 1; });
    tweak!("raw: no content", |ds| { ds.raw[0].content = None; });
    tweak!("raw: block position", |ds| { ds.raw[1].position = pos(99); });

    // @SECURITY
    tweak!("security: no section", |ds| { ds.security = None; });
    tweak!("security: section position", |ds| { ds.security.as_mut().unwrap().position = pos(99); });
    tweak!("security: block key", |ds| { ds.security.as_mut().unwrap().entries[0].block_key.push('x'); });
    tweak!("security: field key", |ds| { ds.security.as_mut().unwrap().entries[0].fields[0].key.push('x'); });
    tweak!("security: field value", |ds| { ds.security.as_mut().unwrap().entries[0].fields[1].value = int(100, 7); });
    tweak!("security: field position", |ds| { ds.security.as_mut().unwrap().entries[0].fields[0].position = pos(99); });
    tweak!("security: one field fewer", |ds| { ds.security.as_mut().unwrap().entries[0].fields.pop(); });
    tweak!("security: entry position", |ds| { ds.security.as_mut().unwrap().entries[0].position = pos(99); });

    // @SCHEMA
    tweak!("schema: no section", |ds| { ds.schema = None; });
    tweak!("schema: section position", |ds| { ds.schema.as_mut().unwrap().position = pos(99); });
    tweak!("schema: path", |ds| { ds.schema.as_mut().unwrap().fields[0].path.segments.push("z".into()); });
    tweak!("schema: method", |ds| { ds.schema.as_mut().unwrap().fields[0].method.push('x'); });
    tweak!("schema: argument", |ds| { ds.schema.as_mut().unwrap().fields[0].arguments[1] = int(100, 8); });
    tweak!("schema: one argument fewer", |ds| { ds.schema.as_mut().unwrap().fields[0].arguments.pop(); });
    tweak!("schema: field position", |ds| { ds.schema.as_mut().unwrap().fields[0].position = pos(99); });

    // @QUICKFUNCS
    tweak!("quick: no section", |ds| { ds.quick_functions = None; });
    tweak!("quick: section position", |ds| { ds.quick_functions.as_mut().unwrap().position = pos(99); });
    tweak!("quick: one function fewer", |ds| { ds.quick_functions.as_mut().unwrap().functions.pop(); });
    tweak!("quick: name", |ds| { ds.quick_functions.as_mut().unwrap().functions[0].name.push('x'); });
    tweak!("quick: return type", |ds| { ds.quick_functions.as_mut().unwrap().functions[0].return_type = None; });
    tweak!("quick: scope list gone", |ds| { ds.quick_functions.as_mut().unwrap().functions[0].scope_list = None; });
    tweak!("quick: scope list entry", |ds| {
        ds.quick_functions.as_mut().unwrap().functions[0].scope_list.as_mut().unwrap()[1].push('x');
    });
    tweak!("quick: none scope list becomes an empty one", |ds| {
        ds.quick_functions.as_mut().unwrap().functions[1].scope_list = Some(vec![]);
    });
    tweak!("quick: param name", |ds| { ds.quick_functions.as_mut().unwrap().functions[0].parameters[0].name.push('x'); });
    tweak!("quick: param data type", |ds| { ds.quick_functions.as_mut().unwrap().functions[0].parameters[0].data_type = None; });
    tweak!("quick: param default", |ds| { ds.quick_functions.as_mut().unwrap().functions[0].parameters[0].default_value = Some(ident("other", 9)); });
    tweak!("quick: param default gone", |ds| { ds.quick_functions.as_mut().unwrap().functions[0].parameters[0].default_value = None; });
    tweak!("quick: param default appears", |ds| { ds.quick_functions.as_mut().unwrap().functions[1].parameters[0].default_value = Some(ident("d", 10)); });
    tweak!("quick: param position", |ds| { ds.quick_functions.as_mut().unwrap().functions[0].parameters[0].position = pos(99); });
    tweak!("quick: body statement", |ds| {
        ds.quick_functions.as_mut().unwrap().functions[0].body[0] =
            QuickFuncStatement::Return { value: ident("other", 9), position: pos(9) };
    });
    tweak!("quick: statement appears", |ds| {
        ds.quick_functions.as_mut().unwrap().functions[1].body.push(QuickFuncStatement::Log { value: ident("l", 10), position: pos(10) });
    });
    tweak!("quick: function position", |ds| { ds.quick_functions.as_mut().unwrap().functions[1].position = pos(99); });

    out
}

#[test]
fn every_compared_field_is_actually_compared() {
    let base = base();
    let base_doc = Document::from_owned(&base);
    let variants = one_field_apart();
    assert!(variants.len() >= 60, "expected a tweak for every field, found {}", variants.len());

    for (label, variant) in &variants {
        assert!(*variant != base, "{label}: the tweak did not change the owned file");
        let doc = Document::from_owned(variant);
        assert_eq!(doc.export(), *variant, "{label}: round trip");
        assert!(doc != base_doc, "{label}: documents compare equal");
        assert!(base_doc != doc, "{label}: documents compare equal (reversed)");
        assert!(doc == Document::from_owned(variant), "{label}: a document is not equal to a copy of itself");
    }
}

// -----------------------------------------------------------------------------
// Real compiler output for the sections the corpus does not contain
// -----------------------------------------------------------------------------

const RAW_SOURCE: &str = r#"
@RAW(
  meta_data -> { id = "atlas_a", format = "MPX" }
  using -> { compression = "gzip", threads = 2 }
  content -> {
---atag
AAAA
---atag
  }
)
@RAW(
  meta_data -> { id = "atlas_b", format = "MPX" }
  content -> {
---btag
BBBB
---btag
  }
)
"#;

const SCHEMA_SOURCE: &str = r#"
@DATA(
  port = 8080
  database.primary: host = "db.local", port = 5432
)
@SCHEMA(
  port = Schema.Int(true, 1, 65535)
  database.primary.host = Schema.String(true)
  database.primary.port = Schema.Int(false)
)
"#;

#[test]
fn compiled_raw_and_schema_sections_round_trip() {
    // No file in `mdix_files/` has an @RAW or @SCHEMA section, so these two sections would
    // otherwise be covered only by the hand-built file. These sources are the ones the raw
    // and schema test files already compile.
    let raw = DixLoader::new_silent().compile_to_resolved_ast_from_str(RAW_SOURCE, "arena-raw").expect("raw source compiles");
    let schema = DixLoader::new_silent().compile_to_resolved_ast_from_str(SCHEMA_SOURCE, "arena-schema").expect("schema source compiles");
    assert_eq!(raw.raw.len(), 2);
    assert!(raw.raw[0].content.is_some() && !raw.raw[0].using.is_empty());
    assert_eq!(schema.schema.as_ref().expect("schema section").fields.len(), 3);

    for (label, ds) in [("raw", &raw), ("schema", &schema)] {
        let doc = Document::from_owned(ds);
        assert_eq!(doc.export(), *ds, "{label}: round trip");
        assert!(doc == Document::from_owned(ds), "{label}: not equal to a copy of itself");
        assert!(doc == doc.clone(), "{label}: not equal to its clone");
    }
    assert!(Document::from_owned(&raw) != Document::from_owned(&schema));
}

// -----------------------------------------------------------------------------
// The corpus
// -----------------------------------------------------------------------------

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
fn every_compiled_file_in_the_corpus_round_trips_whole() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("mdix_files");
    let mut files = Vec::new();
    walk(&root, &mut files);
    files.sort();
    assert!(files.len() >= 50, "expected the corpus under {}, found {} files", root.display(), files.len());

    let mut compiled: Vec<(PathBuf, DixScript, Document)> = Vec::new();
    for path in &files {
        let Ok(ds) = DixLoader::new_silent().compile_to_resolved_ast(path.to_str().unwrap()) else { continue };
        let doc = Document::from_owned(&ds);
        assert_eq!(doc.export(), ds, "round trip, {}", path.display());
        assert!(doc == Document::from_owned(&ds), "a document differs from a second copy of itself, {}", path.display());
        assert!(doc == doc.clone(), "a document differs from its clone, {}", path.display());
        compiled.push((path.clone(), ds, doc));
    }

    // Consecutive files: the documents must give the owned answer. Almost all pairs are
    // different, which guards against an equality that says yes to everything.
    let mut unequal_pairs = 0;
    for pair in compiled.windows(2) {
        let ((pa, a, da), (_, b, db)) = (&pair[0], &pair[1]);
        let owned = a == b;
        assert_eq!(*da == *db, owned, "{}", pa.display());
        if !owned {
            unequal_pairs += 1;
        }
    }

    let with_data = compiled.iter().filter(|(_, ds, _)| ds.data.is_some()).count();
    let with_raw = compiled.iter().filter(|(_, ds, _)| !ds.raw.is_empty()).count();
    let with_security = compiled.iter().filter(|(_, ds, _)| ds.security.is_some()).count();
    let with_schema = compiled.iter().filter(|(_, ds, _)| ds.schema.is_some()).count();
    let with_quick = compiled.iter().filter(|(_, ds, _)| ds.quick_functions.is_some()).count();
    let (mut simple, mut table, mut group, mut object) = (0, 0, 0, 0);
    for (_, ds, _) in &compiled {
        for entry in ds.data.iter().flat_map(|d| d.entries.iter()) {
            match entry {
                DataEntry::SimpleProperty { .. } => simple += 1,
                DataEntry::TableProperty { .. } => table += 1,
                DataEntry::GroupArray { .. } => group += 1,
                DataEntry::ObjectProperty { .. } => object += 1,
            }
        }
    }
    println!(
        "corpus: {} files, {} compiled and round-tripped whole; sections present: data {}, raw {}, security {}, schema {}, quickfuncs {}; data entries: simple {}, table {}, group {}, object {}; {} unequal neighbouring pairs",
        files.len(), compiled.len(), with_data, with_raw, with_security, with_schema, with_quick, simple, table, group, object, unequal_pairs
    );
    assert!(compiled.len() >= 40, "only {} corpus files compiled", compiled.len());
    assert!(with_data > 0 && with_quick > 0, "the corpus must exercise @DATA and @QUICKFUNCS");
    assert!(unequal_pairs >= 20, "too few unequal pairs to test equality");
}
