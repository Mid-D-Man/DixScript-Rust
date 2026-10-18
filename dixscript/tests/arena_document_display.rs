// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/arena-ast.md, section "tests/arena_document_display.rs"
// ============================================================================
//! Proves that `Display` for the arena-resident `Document` and for the `Doc*` section twins
//! prints exactly what the owned `DixScript` and its section types print.
//!
//! The owned `Display` output is the reference. A hand-built file fills every value-holding
//! section with each variant and each optional part, and the checks compare the whole file, every
//! section, and every entry, field, function and parameter inside the sections. Edge cases
//! (empty sections, an entry with no fields) cover the separators. Changing exactly one printed
//! field at a time shows that each one reaches the output. Real compiler output covers `@RAW`
//! and `@SCHEMA`, and every file in `mdix_files/` that compiles is checked whole.

use dixscript::Compiler::AST::arena::{Arena, DocData, DocQuickFuncs, DocRawBlock, DocSchema, DocSecurity, Document};
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

/// A file with every value-holding section filled, every entry variant present, and the optional
/// parts present in some places and absent in others.
fn base() -> DixScript {
    let mut ds = DixScript::new();

    ds.data = Some(DataSection::new(
        vec![
            DataEntry::SimpleProperty { name: "a".into(), data_type: Some(DataType::Int), value: int(1, 1), position: pos(1) },
            DataEntry::SimpleProperty { name: "b".into(), data_type: None, value: text("plain", 1), position: pos(1) },
            DataEntry::TableProperty {
                path: path(&["t", "u"]),
                properties: vec![
                    PropertyAssignment::new("x".into(), Some(DataType::Long), int(2, 2), pos(2)),
                    PropertyAssignment::new("y".into(), None, text("s", 2), pos(3)),
                ],
                position: pos(2),
            },
            DataEntry::GroupArray { path: path(&["g"]), items: vec![int(3, 3), int(4, 3), int(5, 3)], position: pos(3) },
            DataEntry::ObjectProperty {
                name: "o".into(),
                data_type: Some(DataType::Int),
                object: Box::new(Value::Object { properties: vec![], position: pos(4) }),
                position: pos(4),
            },
            DataEntry::ObjectProperty {
                name: "p".into(),
                data_type: None,
                object: Box::new(Value::Object { properties: vec![], position: pos(4) }),
                position: pos(4),
            },
        ],
        pos(1),
    ));

    ds.raw = vec![
        RawBlock::new(
            vec![RawField::new("id".into(), text("blob", 5), pos(5)), RawField::new("format".into(), text("MPX", 5), pos(5))],
            vec![RawField::new("n".into(), int(7, 5), pos(5)), RawField::new("m".into(), int(8, 5), pos(5))],
            Some(RawContent::new("tag".into(), 3, 9, pos(5))),
            pos(5),
        ),
        RawBlock::new(vec![], vec![], None, pos(6)),
    ];

    ds.security = Some(SecuritySection::new(
        vec![
            SecurityEntry::new(
                "k".into(),
                vec![SecurityField::new("f".into(), text("v", 7), pos(7)), SecurityField::new("g".into(), int(9, 7), pos(7))],
                pos(7),
            ),
            SecurityEntry::new("j".into(), vec![SecurityField::new("h".into(), int(10, 7), pos(7))], pos(7)),
        ],
        pos(7),
    ));

    ds.schema = Some(SchemaBlock::new(
        vec![
            SchemaField::new(path(&["t", "x"]), "range".into(), vec![int(1, 8), int(5, 8), int(9, 8)], pos(8)),
            SchemaField::new(path(&["z"]), "Int".into(), vec![], pos(8)),
        ],
        pos(8),
    ));

    ds.quick_functions = Some(QuickFuncsSection::new(
        vec![
            QuickFunction::new(
                "f".into(),
                Some(DataType::Int),
                Some(vec!["main".into(), "aux".into()]),
                vec![
                    QuickFuncParam::new("p".into(), Some(DataType::Int), Some(ident("d", 9)), pos(9)),
                    QuickFuncParam::new("r".into(), None, None, pos(9)),
                ],
                vec![
                    QuickFuncStatement::Assignment { variable: "v".into(), value: ident("p", 9), position: pos(9) },
                    QuickFuncStatement::Return { value: ident("p", 9), position: pos(9) },
                ],
                pos(9),
            ),
            QuickFunction::new(
                "g".into(),
                None,
                None,
                vec![QuickFuncParam::new("q".into(), None, None, pos(10))],
                vec![QuickFuncStatement::Log { value: ident("q", 10), position: pos(10) }],
                pos(10),
            ),
        ],
        pos(9),
    ));

    ds
}

// -----------------------------------------------------------------------------
// The comparison helpers
// -----------------------------------------------------------------------------

/// Asserts that two printed forms are identical, naming the item when they are not.
fn same(label: &str, arena_text: String, owned_text: String) {
    assert_eq!(arena_text, owned_text, "{label}");
}

fn check_data(a: &Arena, doc: &DocData, owned: &DataSection, label: &str) -> usize {
    same(&format!("{label}: @DATA section"), a.display(doc).to_string(), owned.to_string());
    let mut items = 1;
    assert_eq!(doc.entries.len(), owned.entries.len());
    for (i, (d, o)) in doc.entries.iter().zip(&owned.entries).enumerate() {
        same(&format!("{label}: @DATA entry {i}"), a.display(d).to_string(), o.to_string());
        items += 1;
        if let (dixscript::Compiler::AST::arena::DocDataEntry::TableProperty { properties: dp, .. }, DataEntry::TableProperty { properties: op, .. }) = (d, o) {
            assert_eq!(dp.len(), op.len());
            for (j, (dprop, oprop)) in dp.iter().zip(op).enumerate() {
                same(&format!("{label}: @DATA entry {i} property {j}"), a.display(dprop).to_string(), oprop.to_string());
                items += 1;
            }
        }
    }
    items
}

fn check_raw(a: &Arena, doc: &DocRawBlock, owned: &RawBlock, label: &str) -> usize {
    same(&format!("{label}: @RAW block"), a.display(doc).to_string(), owned.to_string());
    let mut items = 1;
    for (name, d, o) in [("meta_data", &doc.meta_data, &owned.meta_data), ("using", &doc.using, &owned.using)] {
        assert_eq!(d.len(), o.len());
        for (i, (df, of)) in d.iter().zip(o).enumerate() {
            same(&format!("{label}: @RAW {name} field {i}"), a.display(df).to_string(), of.to_string());
            items += 1;
        }
    }
    items
}

fn check_security(a: &Arena, doc: &DocSecurity, owned: &SecuritySection, label: &str) -> usize {
    same(&format!("{label}: @SECURITY section"), a.display(doc).to_string(), owned.to_string());
    let mut items = 1;
    assert_eq!(doc.entries.len(), owned.entries.len());
    for (i, (d, o)) in doc.entries.iter().zip(&owned.entries).enumerate() {
        same(&format!("{label}: @SECURITY entry {i}"), a.display(d).to_string(), o.to_string());
        items += 1;
        assert_eq!(d.fields.len(), o.fields.len());
        for (j, (df, of)) in d.fields.iter().zip(&o.fields).enumerate() {
            same(&format!("{label}: @SECURITY entry {i} field {j}"), a.display(df).to_string(), of.to_string());
            items += 1;
        }
    }
    items
}

fn check_schema(a: &Arena, doc: &DocSchema, owned: &SchemaBlock, label: &str) -> usize {
    same(&format!("{label}: @SCHEMA section"), a.display(doc).to_string(), owned.to_string());
    let mut items = 1;
    assert_eq!(doc.fields.len(), owned.fields.len());
    for (i, (d, o)) in doc.fields.iter().zip(&owned.fields).enumerate() {
        same(&format!("{label}: @SCHEMA field {i}"), a.display(d).to_string(), o.to_string());
        items += 1;
    }
    items
}

fn check_quickfuncs(a: &Arena, doc: &DocQuickFuncs, owned: &QuickFuncsSection, label: &str) -> usize {
    same(&format!("{label}: @QUICKFUNCS section"), a.display(doc).to_string(), owned.to_string());
    let mut items = 1;
    assert_eq!(doc.functions.len(), owned.functions.len());
    for (i, (d, o)) in doc.functions.iter().zip(&owned.functions).enumerate() {
        same(&format!("{label}: @QUICKFUNCS function {i}"), a.display(d).to_string(), o.to_string());
        items += 1;
        assert_eq!(d.parameters.len(), o.parameters.len());
        for (j, (dp, op)) in d.parameters.iter().zip(&o.parameters).enumerate() {
            same(&format!("{label}: @QUICKFUNCS function {i} parameter {j}"), a.display(dp).to_string(), op.to_string());
            items += 1;
        }
    }
    items
}

/// Checks the whole file and every section and item in it. Returns how many printed items were
/// compared, so a caller can see that the check was not vacuous.
fn check_document(label: &str, ds: &DixScript) -> usize {
    let doc = Document::from_owned(ds);
    same(&format!("{label}: whole file"), doc.to_string(), ds.to_string());
    same(&format!("{label}: clone of the document"), doc.clone().to_string(), ds.to_string());

    let a = &doc.arena;
    let mut items = 1;
    if let (Some(d), Some(o)) = (&doc.data, &ds.data) {
        items += check_data(a, d, o, label);
    }
    assert_eq!(doc.raw.len(), ds.raw.len());
    for (d, o) in doc.raw.iter().zip(&ds.raw) {
        items += check_raw(a, d, o, label);
    }
    if let (Some(d), Some(o)) = (&doc.security, &ds.security) {
        items += check_security(a, d, o, label);
    }
    if let (Some(d), Some(o)) = (&doc.schema, &ds.schema) {
        items += check_schema(a, d, o, label);
    }
    if let (Some(d), Some(o)) = (&doc.quick_functions, &ds.quick_functions) {
        items += check_quickfuncs(a, d, o, label);
    }
    items
}

// -----------------------------------------------------------------------------
// Hand-built files
// -----------------------------------------------------------------------------

/// The hand-built file and a document made from it.
fn base_pair() -> (DixScript, Document) {
    let ds = base();
    let doc = Document::from_owned(&ds);
    (ds, doc)
}

#[test]
fn a_hand_built_data_section_prints_exactly_like_the_owned_one() {
    let (ds, doc) = base_pair();
    // The section, 6 entries and 2 table properties.
    let items = check_data(&doc.arena, doc.data.as_ref().unwrap(), ds.data.as_ref().unwrap(), "hand-built");
    assert_eq!(items, 1 + 6 + 2);
}

#[test]
fn hand_built_raw_blocks_print_exactly_like_the_owned_ones() {
    let (ds, doc) = base_pair();
    // Two blocks and four fields in the first.
    let items: usize = doc.raw.iter().zip(&ds.raw).map(|(d, o)| check_raw(&doc.arena, d, o, "hand-built")).sum();
    assert_eq!(items, 2 + 4);
}

#[test]
fn a_hand_built_security_section_prints_exactly_like_the_owned_one() {
    let (ds, doc) = base_pair();
    // The section, 2 entries and 3 fields.
    let items = check_security(&doc.arena, doc.security.as_ref().unwrap(), ds.security.as_ref().unwrap(), "hand-built");
    assert_eq!(items, 1 + 2 + 3);
}

#[test]
fn a_hand_built_schema_section_prints_exactly_like_the_owned_one() {
    let (ds, doc) = base_pair();
    // The section and 2 fields.
    let items = check_schema(&doc.arena, doc.schema.as_ref().unwrap(), ds.schema.as_ref().unwrap(), "hand-built");
    assert_eq!(items, 1 + 2);
}

#[test]
fn a_hand_built_quickfuncs_section_prints_exactly_like_the_owned_one() {
    let (ds, doc) = base_pair();
    // The section, 2 functions and 3 parameters.
    let items = check_quickfuncs(&doc.arena, doc.quick_functions.as_ref().unwrap(), ds.quick_functions.as_ref().unwrap(), "hand-built");
    assert_eq!(items, 1 + 2 + 3);
}

#[test]
fn a_hand_built_file_prints_exactly_like_the_owned_file() {
    let (ds, doc) = base_pair();
    let printed = ds.to_string();
    for header in ["@QUICKFUNCS(", "@DATA(", "@SECURITY(", "@RAW(", "@SCHEMA("] {
        assert!(printed.contains(header), "the sample should print {header}");
    }
    assert_eq!(doc.to_string(), printed);
    assert_eq!(doc.clone().to_string(), printed);
}

#[test]
fn an_empty_file_and_empty_sections_print_exactly_like_the_owned_ones() {
    let empty = DixScript::new();
    assert_eq!(empty.to_string(), "");
    check_document("empty file", &empty);

    let mut with_empty_sections = DixScript::new();
    with_empty_sections.data = Some(DataSection::new(vec![], pos(1)));
    with_empty_sections.security = Some(SecuritySection::new(vec![], pos(1)));
    with_empty_sections.schema = Some(SchemaBlock::new(vec![], pos(1)));
    with_empty_sections.quick_functions = Some(QuickFuncsSection::new(vec![], pos(1)));
    with_empty_sections.raw = vec![RawBlock::new(vec![], vec![], None, pos(1))];
    check_document("empty sections", &with_empty_sections);
}

#[test]
fn empty_collections_inside_entries_print_exactly_like_the_owned_ones() {
    let mut ds = DixScript::new();
    ds.data = Some(DataSection::new(
        vec![
            DataEntry::TableProperty { path: path(&["t"]), properties: vec![], position: pos(1) },
            DataEntry::GroupArray { path: path(&["g"]), items: vec![], position: pos(2) },
        ],
        pos(1),
    ));
    ds.security = Some(SecuritySection::new(
        vec![SecurityEntry::new("k".into(), vec![], pos(3)), SecurityEntry::new("j".into(), vec![], pos(3))],
        pos(3),
    ));
    ds.quick_functions = Some(QuickFuncsSection::new(
        vec![QuickFunction::new("empty".into(), None, None, vec![], vec![], pos(4))],
        pos(4),
    ));
    ds.schema = Some(SchemaBlock::new(vec![SchemaField::new(path(&["a"]), "Int".into(), vec![], pos(5))], pos(5)));
    check_document("empty collections", &ds);
}

/// Each variant changes exactly one field that the owned `Display` prints.
fn one_printed_field_apart() -> Vec<(&'static str, fn(&mut DixScript))> {
    fn data(ds: &mut DixScript) -> &mut Vec<DataEntry> {
        &mut ds.data.as_mut().unwrap().entries
    }
    fn raw(ds: &mut DixScript, i: usize) -> &mut RawBlock {
        &mut ds.raw[i]
    }
    fn sec(ds: &mut DixScript) -> &mut Vec<SecurityEntry> {
        &mut ds.security.as_mut().unwrap().entries
    }
    fn schema(ds: &mut DixScript) -> &mut Vec<SchemaField> {
        &mut ds.schema.as_mut().unwrap().fields
    }
    fn funcs(ds: &mut DixScript) -> &mut Vec<QuickFunction> {
        &mut ds.quick_functions.as_mut().unwrap().functions
    }
    fn v(name: &'static str, apply: fn(&mut DixScript)) -> (&'static str, fn(&mut DixScript)) {
        (name, apply)
    }

    vec![
        v("data simple name", |ds| { if let DataEntry::SimpleProperty { name, .. } = &mut data(ds)[0] { name.push('2'); } }),
        v("data simple type", |ds| { if let DataEntry::SimpleProperty { data_type, .. } = &mut data(ds)[0] { *data_type = Some(DataType::Long); } }),
        v("data simple type removed", |ds| { if let DataEntry::SimpleProperty { data_type, .. } = &mut data(ds)[0] { *data_type = None; } }),
        v("data simple type added", |ds| { if let DataEntry::SimpleProperty { data_type, .. } = &mut data(ds)[1] { *data_type = Some(DataType::Int); } }),
        v("data simple value", |ds| { if let DataEntry::SimpleProperty { value, .. } = &mut data(ds)[0] { *value = int(99, 1); } }),
        v("data table path", |ds| { if let DataEntry::TableProperty { path, .. } = &mut data(ds)[2] { path.segments.push("w".into()); } }),
        v("data table property name", |ds| { if let DataEntry::TableProperty { properties, .. } = &mut data(ds)[2] { properties[0].name.push('2'); } }),
        v("data table property type", |ds| { if let DataEntry::TableProperty { properties, .. } = &mut data(ds)[2] { properties[0].data_type = None; } }),
        v("data table property type added", |ds| { if let DataEntry::TableProperty { properties, .. } = &mut data(ds)[2] { properties[1].data_type = Some(DataType::Int); } }),
        v("data table property value", |ds| { if let DataEntry::TableProperty { properties, .. } = &mut data(ds)[2] { properties[1].value = text("other", 2); } }),
        v("data group path", |ds| { if let DataEntry::GroupArray { path, .. } = &mut data(ds)[3] { path.segments[0].push('2'); } }),
        v("data group item", |ds| { if let DataEntry::GroupArray { items, .. } = &mut data(ds)[3] { items[2] = int(77, 3); } }),
        v("data group item removed", |ds| { if let DataEntry::GroupArray { items, .. } = &mut data(ds)[3] { items.pop(); } }),
        v("data object name", |ds| { if let DataEntry::ObjectProperty { name, .. } = &mut data(ds)[4] { name.push('2'); } }),
        v("data object type", |ds| { if let DataEntry::ObjectProperty { data_type, .. } = &mut data(ds)[4] { *data_type = None; } }),
        v("data object type added", |ds| { if let DataEntry::ObjectProperty { data_type, .. } = &mut data(ds)[5] { *data_type = Some(DataType::Long); } }),
        v("data object value", |ds| { if let DataEntry::ObjectProperty { object, .. } = &mut data(ds)[4] { **object = int(5, 4); } }),
        v("raw meta key", |ds| { raw(ds, 0).meta_data[0].key.push('2'); }),
        v("raw meta value", |ds| { raw(ds, 0).meta_data[1].value = text("other", 5); }),
        v("raw using key", |ds| { raw(ds, 0).using[0].key.push('2'); }),
        v("raw using value", |ds| { raw(ds, 0).using[1].value = int(55, 5); }),
        v("raw content tag", |ds| { if let Some(c) = &mut raw(ds, 0).content { c.tag.push('2'); } }),
        v("raw content length", |ds| { if let Some(c) = &mut raw(ds, 0).content { c.end += 11; } }),
        v("raw content added", |ds| { raw(ds, 1).content = Some(RawContent::new("t".into(), 0, 4, pos(6))); }),
        v("raw content removed", |ds| { raw(ds, 0).content = None; }),
        v("security block key", |ds| { sec(ds)[0].block_key.push('2'); }),
        v("security field key", |ds| { sec(ds)[0].fields[1].key.push('2'); }),
        v("security field value", |ds| { sec(ds)[1].fields[0].value = text("other", 7); }),
        v("security field removed", |ds| { sec(ds)[0].fields.pop(); }),
        v("security entry removed", |ds| { sec(ds).pop(); }),
        v("schema path", |ds| { schema(ds)[0].path.segments.push("w".into()); }),
        v("schema method", |ds| { schema(ds)[1].method = "String".into(); }),
        v("schema argument", |ds| { schema(ds)[0].arguments[1] = int(66, 8); }),
        v("schema argument removed", |ds| { schema(ds)[0].arguments.pop(); }),
        v("quickfunc name", |ds| { funcs(ds)[0].name.push('2'); }),
        v("quickfunc return type", |ds| { funcs(ds)[0].return_type = None; }),
        v("quickfunc return type added", |ds| { funcs(ds)[1].return_type = Some(DataType::Int); }),
        v("quickfunc scope list", |ds| { funcs(ds)[0].scope_list = Some(vec!["main".into()]); }),
        v("quickfunc scope list removed", |ds| { funcs(ds)[0].scope_list = None; }),
        v("quickfunc scope list added", |ds| { funcs(ds)[1].scope_list = Some(vec!["x".into()]); }),
        v("quickfunc param name", |ds| { funcs(ds)[0].parameters[0].name.push('2'); }),
        v("quickfunc param type", |ds| { funcs(ds)[0].parameters[0].data_type = None; }),
        v("quickfunc param type added", |ds| { funcs(ds)[0].parameters[1].data_type = Some(DataType::Long); }),
        v("quickfunc param default", |ds| { funcs(ds)[0].parameters[0].default_value = Some(ident("other", 9)); }),
        v("quickfunc param default removed", |ds| { funcs(ds)[0].parameters[0].default_value = None; }),
        v("quickfunc param default added", |ds| { funcs(ds)[0].parameters[1].default_value = Some(ident("d", 9)); }),
        v("quickfunc param removed", |ds| { funcs(ds)[0].parameters.pop(); }),
        v("quickfunc statement", |ds| { funcs(ds)[0].body[1] = QuickFuncStatement::Return { value: ident("other", 9), position: pos(9) }; }),
        v("quickfunc statement removed", |ds| { funcs(ds)[0].body.pop(); }),
        v("quickfunc removed", |ds| { funcs(ds).pop(); }),
    ]
}

#[test]
fn every_printed_field_reaches_the_output() {
    let reference = base().to_string();
    let variants = one_printed_field_apart();
    assert!(variants.len() >= 45, "expected the full set of variants, found {}", variants.len());

    for (name, apply) in variants {
        let mut ds = base();
        apply(&mut ds);
        let owned = ds.to_string();
        // The change must be one that the owned `Display` prints, or the check proves nothing.
        assert_ne!(owned, reference, "{name}: the owned output did not change, so the variant tests nothing");
        let doc = Document::from_owned(&ds);
        assert_eq!(doc.to_string(), owned, "{name}: whole file");
        check_document(name, &ds);
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
fn compiled_raw_and_schema_sources_print_exactly_like_the_owned_file() {
    let raw = DixLoader::new_silent().compile_to_resolved_ast_from_str(RAW_SOURCE, "display-raw").expect("raw source compiles");
    let schema = DixLoader::new_silent().compile_to_resolved_ast_from_str(SCHEMA_SOURCE, "display-schema").expect("schema source compiles");
    assert_eq!(raw.raw.len(), 2);
    assert_eq!(schema.schema.as_ref().expect("schema section").fields.len(), 3);

    let raw_items = check_document("raw", &raw);
    let schema_items = check_document("schema", &schema);
    // The raw file: the file (1), 2 blocks, and 6 fields (4 in the first block, 2 in the second).
    // The schema file: the file (1), @DATA (section, 2 entries and 2 table properties: 5), and
    // @SCHEMA (section and 3 fields: 4).
    assert_eq!(raw_items, 1 + 2 + 6);
    assert_eq!(schema_items, 1 + 5 + 4);
    assert!(raw.to_string().contains("content -> { ---atag--- <5 bytes> ---atag--- }"));
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
fn every_compiled_file_in_the_corpus_prints_exactly_like_the_owned_file() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("mdix_files");
    let mut files = Vec::new();
    walk(&root, &mut files);
    files.sort();
    assert!(files.len() >= 50, "expected the corpus under {}, found {} files", root.display(), files.len());

    let (mut compiled, mut items) = (0usize, 0usize);
    let (mut with_config, mut with_data, mut with_quickfuncs, mut with_security) = (0usize, 0usize, 0usize, 0usize);
    let mut total_chars = 0usize;
    for path in &files {
        let Ok(ds) = DixLoader::new_silent().compile_to_resolved_ast(path.to_str().unwrap()) else { continue };
        compiled += 1;
        items += check_document(&path.display().to_string(), &ds);
        with_config += ds.config.is_some() as usize;
        with_data += ds.data.is_some() as usize;
        with_quickfuncs += ds.quick_functions.is_some() as usize;
        with_security += ds.security.is_some() as usize;
        total_chars += ds.to_string().len();
    }

    // Guard against a corpus run that compared nothing. 70 files compile without network access,
    // with 70 @CONFIG, 65 @DATA, 40 @QUICKFUNCS and 12 @SECURITY sections; a run with network
    // access compiles more.
    assert!(compiled >= 60, "only {compiled} corpus files compiled");
    assert!(
        with_config >= 60 && with_data >= 55 && with_quickfuncs >= 30 && with_security >= 10,
        "the corpus should exercise every section: config {with_config}, data {with_data}, quickfuncs {with_quickfuncs}, security {with_security}"
    );
    assert!(items > 5_000, "only {items} items compared across {compiled} files");
    assert!(total_chars > 400_000, "the corpus printed only {total_chars} characters");
}
