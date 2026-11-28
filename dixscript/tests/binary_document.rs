// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/compiler.md, section "tests/binary_document.rs"
// ============================================================================
//! The binary packer reads the arena `Document`, and the owned writers are its oracle.
//!
//! `DataSectionWriter::write_section` and `SecuritySectionWriter::write_section` still take
//! the owned section types and still run the original code; `write_document_section` is the
//! arena version of each. Over the same tree the two have to agree on everything an outside
//! reader can see: the bytes, the statistics, and the text of the error. The tests here
//! compare them over every corpus file that compiles and over hand-built trees that reach each
//! type tag and each error path of the value encoder.

use std::collections::BTreeMap;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use dixscript::Compiler::AST::arena::{Arena, Document};
use dixscript::Compiler::AST::{
    DataEntry, DataSection, DixScript, EnumDeclaration, EnumField, EnumsSection, ObjectProperty,
    Position, PropertyAssignment, SecurityEntry, SecurityField, SecuritySection, TablePath, Value,
};
use dixscript::Compiler::Core::BinarySerialization::{
    BinaryHeader, BinaryPacker, BinaryUnpacker, BinarySerializationContext, ChecksumValidator,
    DataSectionWriter, SectionOffset, SecuritySectionWriter, ValueDecoder, ValueEncoder,
    ValueTypeTag, MAX_ARRAY_LENGTH, MAX_NESTING_DEPTH, MAX_OBJECT_PROPERTIES, MAX_STRING_LENGTH,
};
use dixscript::Compiler::Utilities::SecurityUtilities;
use dixscript::Runtime::DixLoader;

const P: Position = Position::UNKNOWN;

/// What a section writer produced: the bytes and statistics, or the error text.
#[derive(Debug, PartialEq)]
enum Outcome {
    Written {
        bytes: Vec<u8>,
        total_values: usize,
        value_counts: BTreeMap<String, usize>,
        section_sizes: BTreeMap<String, usize>,
    },
    Failed(String),
}

fn outcome(
    result: Result<(), String>,
    bytes: Vec<u8>,
    ctx: BinarySerializationContext,
) -> Outcome {
    match result {
        Ok(()) => Outcome::Written {
            bytes,
            total_values: ctx.statistics.total_values,
            value_counts: ctx.statistics.value_counts.into_iter().collect(),
            section_sizes: ctx.statistics.section_sizes.into_iter().collect(),
        },
        Err(message) => Outcome::Failed(message),
    }
}

fn owned_data(ast: &DixScript) -> Option<Outcome> {
    let data = ast.data.as_ref()?;
    let mut ctx = BinarySerializationContext::new();
    let mut enc = ValueEncoder::new().with_enums(ValueEncoder::build_local_enums(ast.enums.as_ref()));
    let mut buf = Cursor::new(Vec::new());
    let result = DataSectionWriter::new(&mut ctx, &mut enc)
        .write_section(&mut buf, data)
        .map(|_| ())
        .map_err(|e| e.to_string());
    Some(outcome(result, buf.into_inner(), ctx))
}

fn document_data(document: &Document) -> Option<Outcome> {
    let data = document.data.as_ref()?;
    let mut ctx = BinarySerializationContext::new();
    let mut enc =
        ValueEncoder::new().with_enums(ValueEncoder::build_local_enums(document.enums.as_ref()));
    let mut buf = Cursor::new(Vec::new());
    let result = DataSectionWriter::new(&mut ctx, &mut enc)
        .write_document_section(&mut buf, &document.arena, data)
        .map(|_| ())
        .map_err(|e| e.to_string());
    Some(outcome(result, buf.into_inner(), ctx))
}

fn owned_security(ast: &DixScript) -> Option<Outcome> {
    let security = ast.security.as_ref()?;
    let mut ctx = BinarySerializationContext::new();
    let mut enc = ValueEncoder::new();
    let mut buf = Cursor::new(Vec::new());
    let result = SecuritySectionWriter::new(&mut ctx, &mut enc)
        .write_section(&mut buf, security)
        .map(|_| ())
        .map_err(|e| e.to_string());
    Some(outcome(result, buf.into_inner(), ctx))
}

fn document_security(document: &Document) -> Option<Outcome> {
    let security = document.security.as_ref()?;
    let mut ctx = BinarySerializationContext::new();
    let mut enc = ValueEncoder::new();
    let mut buf = Cursor::new(Vec::new());
    let result = SecuritySectionWriter::new(&mut ctx, &mut enc)
        .write_document_section(&mut buf, &document.arena, security)
        .map(|_| ())
        .map_err(|e| e.to_string());
    Some(outcome(result, buf.into_inner(), ctx))
}

/// The two writers over the same tree: `label` names the tree in a failure message.
fn assert_writers_agree(label: &str, ast: &DixScript) {
    let document = Document::from_owned(ast);
    assert_eq!(owned_data(ast), document_data(&document), "{label}: @DATA");
    assert_eq!(owned_security(ast), document_security(&document), "{label}: @SECURITY");
}

/// The sections of a packed file, by id, without the header and the checksum.
fn sections(binary: &[u8]) -> Vec<(String, Vec<u8>)> {
    let data = ChecksumValidator::validate_and_extract(binary).expect("the checksum holds");
    let mut cursor = Cursor::new(data.as_slice());
    let header = BinaryHeader::read_from(&mut cursor).expect("the header reads");
    cursor.set_position(header.offset_table_position as u64);
    (0..header.section_count)
        .map(|_| {
            let offset = SectionOffset::read_from(&mut cursor).expect("an offset entry reads");
            let (from, to) = (offset.offset as usize, (offset.offset + offset.length) as usize);
            (format!("{:?}", offset.section_id), data[from..to].to_vec())
        })
        .collect()
}

fn collect_mdix(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()))
        .map(|entry| entry.unwrap().path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            collect_mdix(&path, out);
        } else if path.extension().map_or(false, |ext| ext == "mdix") {
            out.push(path);
        }
    }
}

/// The corpus files that compile, as documents. The `*_cloud_*` files go to the network and
/// wait for a timeout, so they are left out.
fn corpus_documents() -> Vec<(String, Document)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../mdix_files");
    let mut files = Vec::new();
    collect_mdix(&root, &mut files);
    let loader = DixLoader::new_silent();
    files
        .into_iter()
        .filter(|path| !path.to_string_lossy().contains("_cloud_"))
        .filter_map(|path| {
            let label = path.strip_prefix(&root).unwrap().to_string_lossy().into_owned();
            loader.compile_to_document(&path.to_string_lossy()).ok().map(|d| (label, d))
        })
        .collect()
}

// ---------------------------------------------------------------------------------------------
// The corpus
// ---------------------------------------------------------------------------------------------

#[test]
fn the_document_writers_agree_with_the_owned_writers_over_the_corpus() {
    let documents = corpus_documents();
    assert!(documents.len() >= 60, "only {} corpus files compiled", documents.len());

    let (mut data_sections, mut security_sections, mut failures) = (0, 0, 0);
    for (label, document) in &documents {
        let ast = document.export();
        let (old_data, new_data) = (owned_data(&ast), document_data(document));
        assert_eq!(old_data, new_data, "{label}: @DATA");
        let (old_security, new_security) = (owned_security(&ast), document_security(document));
        assert_eq!(old_security, new_security, "{label}: @SECURITY");

        data_sections += usize::from(new_data.is_some());
        security_sections += usize::from(new_security.is_some());
        failures += usize::from(matches!(new_data, Some(Outcome::Failed(_))));
    }

    // The corpus has to reach every case this test claims to compare: files with a data
    // section, files with a security section, and the files whose data the encoder rejects
    // (an unresolved identifier).
    assert!(data_sections >= 55, "only {data_sections} data sections were compared");
    assert!(security_sections >= 8, "only {security_sections} security sections were compared");
    assert!(failures >= 1, "no corpus file reaches an encoder error");
}

#[test]
fn pack_and_pack_document_write_the_same_sections_over_the_corpus() {
    let documents = corpus_documents();
    let mut packed = 0;
    for (label, document) in &documents {
        let ast = document.export();
        let (from_owned, from_document) =
            (BinaryPacker::new().pack(&ast), BinaryPacker::new().pack_document(document));
        assert_eq!(from_owned.is_success, from_document.is_success, "{label}: verdict");
        assert_eq!(from_owned.errors, from_document.errors, "{label}: errors");
        if !from_document.is_success {
            continue;
        }
        packed += 1;
        assert_eq!(
            sections(&from_owned.binary_data),
            sections(&from_document.binary_data),
            "{label}: sections"
        );
        assert_eq!(from_owned.original_size, from_document.original_size, "{label}: size estimate");
        assert_eq!(
            from_owned.statistics.total_values, from_document.statistics.total_values,
            "{label}: value count"
        );
    }
    assert!(packed >= 55, "only {packed} corpus files packed");
}

// ---------------------------------------------------------------------------------------------
// Hand-built trees
// ---------------------------------------------------------------------------------------------

fn int(value: i32) -> Value {
    Value::Integer { value, position: P }
}

fn string(text: &str) -> Value {
    Value::String { value: text.to_string(), position: P }
}

fn array(values: Vec<Value>) -> Value {
    Value::Array { values, position: P }
}

fn null() -> Value {
    Value::Null { position: P }
}

fn prefixed(prefix: &str, arguments: Vec<Value>) -> Value {
    Value::PrefixedConstructor { prefix: prefix.to_string(), arguments, position: P }
}

fn object(pairs: Vec<(&str, Value)>) -> Value {
    Value::Object {
        properties: pairs
            .into_iter()
            .map(|(key, value)| ObjectProperty { key: key.to_string(), value, position: P })
            .collect(),
        position: P,
    }
}

fn simple(name: &str, value: Value) -> DataEntry {
    DataEntry::SimpleProperty { name: name.to_string(), data_type: None, value, position: P }
}

fn tree(entries: Vec<DataEntry>) -> DixScript {
    let mut ast = DixScript::new();
    ast.data = Some(DataSection::new(entries, P));
    ast
}

fn local_enums() -> EnumsSection {
    let field = |name: &str, value: Option<i32>| EnumField { name: name.to_string(), value, position: P };
    EnumsSection::new(
        vec![EnumDeclaration {
            name: "Color".to_string(),
            fields: vec![field("Red", None), field("Green", Some(10)), field("Blue", None)],
            position: P,
        }],
        P,
    )
}

fn arrays_nested_to(depth: usize) -> Value {
    (0..depth).fold(int(1), |inner, _| array(vec![inner]))
}

#[test]
fn every_value_type_encodes_the_same_through_the_document_writer() {
    let mut ast = tree(vec![
        simple("integer", int(-7)),
        simple("long", Value::Long { value: 1 << 40, position: P }),
        simple("float", Value::Float { value: 1.5, position: P }),
        simple("double", Value::Double { value: -2.25, position: P }),
        simple("scientific", Value::ScientificNotation { value: 1.5e10, position: P }),
        simple("text", string("héllo wörld")),
        simple("empty_text", string("")),
        simple("yes", Value::Boolean { value: true, position: P }),
        simple("no", Value::Boolean { value: false, position: P }),
        simple("nothing", null()),
        simple("hex3", Value::HexColor { value: "#F0A".to_string(), position: P }),
        simple("hex4", Value::HexColor { value: "#F0A8".to_string(), position: P }),
        simple("hex6", Value::HexColor { value: "#FF00AA".to_string(), position: P }),
        simple("hex8", Value::HexColor { value: "#FF00AA80".to_string(), position: P }),
        simple("hex_no_hash", Value::HexColor { value: "FF00AA".to_string(), position: P }),
        simple("date_dash", Value::Date { value: "2024-02-29".to_string(), position: P }),
        simple("date_slash", Value::Date { value: "2024/02/29".to_string(), position: P }),
        simple("stamp", Value::Timestamp { value: "2024-02-29T10:20:30Z".to_string(), position: P }),
        simple("stamp_fraction", Value::Timestamp { value: "2024-02-29T10:20:30.123Z".to_string(), position: P }),
        simple("empty_array", array(vec![])),
        simple("ints", array(vec![int(1), int(2), int(3)])),
        simple("mixed_first_is_int", array(vec![int(1), string("two"), null()])),
        simple("deep", arrays_nested_to(MAX_NESTING_DEPTH - 1)),
        simple("nested_object", object(vec![("a", int(1)), ("b", object(vec![("c", string("d"))])), ("e", array(vec![int(5)]))])),
        simple("empty_object", object(vec![])),
        simple("tuple_one", prefixed("t", vec![int(1)])),
        simple("tuple_six", prefixed("t", (1..=6).map(int).collect())),
        simple("blob_hex", prefixed("b", vec![string("deadBEEF")])),
        simple("blob_base64", prefixed("b", vec![string("aGVsbG8gd29ybGQ=")])),
        simple("blob_base32", prefixed("b", vec![string("NBSWY3DP")])),
        simple("regex", prefixed("r", vec![string("^[a-z]+\\d{2,3}$")])),
        simple("enum_default", Value::EnumValue { enum_name: "Color".to_string(), value: "Red".to_string(), position: P }),
        simple("enum_explicit", Value::EnumValue { enum_name: "Color".to_string(), value: "Green".to_string(), position: P }),
        simple("enum_after_explicit", Value::EnumValue { enum_name: "Color".to_string(), value: "Blue".to_string(), position: P }),
        simple("enum_unknown_enum", Value::EnumValue { enum_name: "Shape".to_string(), value: "Circle".to_string(), position: P }),
        simple("enum_unknown_field", Value::EnumValue { enum_name: "Color".to_string(), value: "Mauve".to_string(), position: P }),
        simple("enum_in_array", array(vec![Value::EnumValue { enum_name: "Color".to_string(), value: "Green".to_string(), position: P }])),
        DataEntry::TableProperty {
            path: TablePath::new(vec!["server".to_string(), "limits".to_string()]),
            properties: vec![
                PropertyAssignment { name: "max".to_string(), data_type: None, value: int(10), position: P },
                PropertyAssignment { name: "tags".to_string(), data_type: None, value: array(vec![string("a"), string("b")]), position: P },
            ],
            position: P,
        },
        DataEntry::TableProperty { path: TablePath::new(vec!["empty".to_string()]), properties: vec![], position: P },
        DataEntry::GroupArray {
            path: TablePath::new(vec!["rows".to_string()]),
            items: vec![int(1), object(vec![("k", string("v"))]), array(vec![int(2)])],
            position: P,
        },
        DataEntry::GroupArray { path: TablePath::new(vec!["none".to_string()]), items: vec![], position: P },
        DataEntry::ObjectProperty {
            name: "settings".to_string(),
            data_type: None,
            object: Box::new(object(vec![("volume", int(11)), ("mode", string("dark"))])),
            position: P,
        },
    ]);
    ast.enums = Some(local_enums());
    ast.security = Some(SecuritySection::new(
        vec![
            SecurityEntry {
                block_key: "encryption".to_string(),
                fields: vec![
                    SecurityField { key: "algorithm".to_string(), value: string("aes256-gcm"), position: P },
                    SecurityField { key: "key_length".to_string(), value: int(32), position: P },
                    SecurityField { key: "salt".to_string(), value: prefixed("b", vec![string("00ff")]), position: P },
                ],
                position: P,
            },
            SecurityEntry { block_key: "empty".to_string(), fields: vec![], position: P },
        ],
        P,
    ));

    assert_writers_agree("every value type", &ast);

    // The case has to be a real success, not two matching failures.
    match document_data(&Document::from_owned(&ast)) {
        Some(Outcome::Written { total_values, value_counts, .. }) => {
            assert!(total_values > 60, "only {total_values} values were encoded");
            for tag in ["int", "long", "float", "double", "string", "bool", "null", "array", "object", "hex", "date", "timestamp", "tuple", "blob", "regex", "enum"] {
                assert!(value_counts.contains_key(tag), "no {tag} value was encoded: {value_counts:?}");
            }
        }
        other => panic!("the all-types tree must encode, got {other:?}"),
    }
}

/// One failing value per entry; each is checked on its own, so a failure cannot hide behind
/// an earlier one.
fn failing_values() -> Vec<(&'static str, Value)> {
    vec![
        ("unknown prefix", Value::PrefixedConstructor { prefix: "x".to_string(), arguments: vec![int(1)], position: Position::new(12, 4) }),
        ("empty tuple", prefixed("t", vec![])),
        ("seven element tuple", prefixed("t", (1..=7).map(int).collect())),
        ("tuple holding a failure", prefixed("t", vec![int(1), prefixed("x", vec![])])),
        ("blob without an argument", prefixed("b", vec![])),
        ("blob with a number", prefixed("b", vec![int(5)])),
        ("blob that is not base64", prefixed("b", vec![string("not base64!!")])),
        ("hex blob of odd length", prefixed("b", vec![string("abc")])),
        ("regex without an argument", prefixed("r", vec![])),
        ("regex with a number", prefixed("r", vec![int(5)])),
        ("regex that does not compile", prefixed("r", vec![string("(")])),
        ("hex colour of five digits", Value::HexColor { value: "#FFFFF".to_string(), position: P }),
        ("hex colour that is not hex", Value::HexColor { value: "#GGG".to_string(), position: P }),
        ("date that is not a date", Value::Date { value: "2024-13-45".to_string(), position: P }),
        ("timestamp that is not a timestamp", Value::Timestamp { value: "yesterday".to_string(), position: P }),
        ("unresolved identifier", Value::Identifier { value: "total".to_string(), position: Position::new(166, 28) }),
        ("nested array node", Value::NestedArray { values: vec![int(1)], level: 2, position: P }),
        ("interpolated string", Value::InterpolatedString { template: "a {b}".to_string(), expressions: vec![], position: P }),
        ("array holding an identifier", array(vec![int(1), Value::Identifier { value: "x".to_string(), position: P }])),
        ("object holding an identifier", object(vec![("k", Value::Identifier { value: "x".to_string(), position: P })])),
        ("too deep", arrays_nested_to(MAX_NESTING_DEPTH + 1)),
        ("too long a string", string(&"x".repeat(MAX_STRING_LENGTH + 1))),
        ("too many elements", array(vec![null(); MAX_ARRAY_LENGTH + 1])),
        (
            "too many properties",
            Value::Object {
                properties: (0..=MAX_OBJECT_PROPERTIES)
                    .map(|i| ObjectProperty { key: format!("k{i}"), value: null(), position: P })
                    .collect(),
                position: P,
            },
        ),
    ]
}

#[test]
fn every_encoder_error_reads_the_same_through_the_document_writer() {
    for (label, value) in failing_values() {
        let mut ast = tree(vec![simple("v", value.clone())]);
        ast.enums = Some(local_enums());
        assert_writers_agree(&format!("{label} as a simple property"), &ast);

        let mut ast = tree(vec![DataEntry::TableProperty {
            path: TablePath::new(vec!["t".to_string()]),
            properties: vec![PropertyAssignment { name: "p".to_string(), data_type: None, value: value.clone(), position: P }],
            position: P,
        }]);
        ast.enums = Some(local_enums());
        assert_writers_agree(&format!("{label} in a table"), &ast);

        let ast = tree(vec![DataEntry::GroupArray {
            path: TablePath::new(vec!["g".to_string()]),
            items: vec![int(1), value.clone()],
            position: P,
        }]);
        assert_writers_agree(&format!("{label} in a group array"), &ast);

        let mut ast = DixScript::new();
        ast.security = Some(SecuritySection::new(
            vec![SecurityEntry {
                block_key: "block".to_string(),
                fields: vec![SecurityField { key: "f".to_string(), value, position: P }],
                position: P,
            }],
            P,
        ));
        assert_writers_agree(&format!("{label} in a security field"), &ast);
    }
}

#[test]
fn the_failing_values_really_fail() {
    // The comparison above would pass if both writers accepted a value, so pin that the
    // encoder rejects every value in the list.
    for (label, value) in failing_values() {
        let mut ast = tree(vec![simple("v", value)]);
        ast.enums = Some(local_enums());
        let outcome = document_data(&Document::from_owned(&ast)).expect("a data section");
        assert!(matches!(outcome, Outcome::Failed(_)), "{label} should be rejected");
    }
}

#[test]
fn an_object_property_whose_value_is_not_an_object_reads_the_same() {
    for value in [int(1), array(vec![int(1)]), string("x"), Value::Identifier { value: "y".to_string(), position: P }] {
        let ast = tree(vec![DataEntry::ObjectProperty {
            name: "o".to_string(),
            data_type: None,
            object: Box::new(value),
            position: P,
        }]);
        assert_writers_agree("object property with a non-object value", &ast);
        match document_data(&Document::from_owned(&ast)) {
            Some(Outcome::Failed(message)) => {
                assert!(message.contains("Expected object literal for property 'o'"), "{message}")
            }
            other => panic!("expected a failure, got {other:?}"),
        }
    }
}


// ---------------------------------------------------------------------------------------------
// The unpacker
// ---------------------------------------------------------------------------------------------

/// What an unpacker produced, in the terms an outside reader can see.
#[derive(Debug, PartialEq)]
enum Unpacked {
    Panicked,
    Failed(Vec<String>),
    Done {
        /// The `Debug` text of the owned tree.
        tree: String,
        total_sections: usize,
        total_values: usize,
        value_counts: BTreeMap<String, usize>,
    },
}

fn owned_unpack(binary: &[u8]) -> Unpacked {
    std::panic::catch_unwind(|| {
        let result = BinaryUnpacker::new().unpack(binary);
        match result.ast {
            Some(ast) if result.is_success => Unpacked::Done {
                tree: format!("{ast:?}"),
                total_sections: result.statistics.total_sections,
                total_values: result.statistics.total_values,
                value_counts: result.statistics.value_counts.into_iter().collect(),
            },
            _ => Unpacked::Failed(result.errors),
        }
    })
    .unwrap_or(Unpacked::Panicked)
}

fn document_unpack(binary: &[u8]) -> Unpacked {
    std::panic::catch_unwind(|| {
        let result = BinaryUnpacker::new().unpack_document(binary);
        match result.document {
            Some(document) if result.is_success => Unpacked::Done {
                tree: format!("{:?}", document.export()),
                total_sections: result.statistics.total_sections,
                total_values: result.statistics.total_values,
                value_counts: result.statistics.value_counts.into_iter().collect(),
            },
            _ => Unpacked::Failed(result.errors),
        }
    })
    .unwrap_or(Unpacked::Panicked)
}

/// A tree with every section the binary format stores and every value type.
fn all_sections_tree() -> DixScript {
    let mut ast = tree(vec![
        simple("integer", int(-7)),
        simple("long", Value::Long { value: 1 << 40, position: P }),
        simple("float", Value::Float { value: 1.5, position: P }),
        simple("double", Value::Double { value: -2.25, position: P }),
        simple("text", string("héllo")),
        simple("yes", Value::Boolean { value: true, position: P }),
        simple("nothing", null()),
        simple("colour", Value::HexColor { value: "#FF00AA80".to_string(), position: P }),
        simple("day", Value::Date { value: "2024-02-29".to_string(), position: P }),
        simple("stamp", Value::Timestamp { value: "2024-02-29T10:20:30Z".to_string(), position: P }),
        simple("list", array(vec![int(1), array(vec![int(2)])])),
        simple("tuple", prefixed("t", vec![int(1), string("a")])),
        simple("blob", prefixed("b", vec![string("deadBEEF")])),
        simple("regex", prefixed("r", vec![string("^a+$")])),
        simple("shade", Value::EnumValue { enum_name: "Color".to_string(), value: "Green".to_string(), position: P }),
        DataEntry::TableProperty {
            path: TablePath::new(vec!["server".to_string(), "limits".to_string()]),
            properties: vec![PropertyAssignment { name: "max".to_string(), data_type: None, value: int(10), position: P }],
            position: P,
        },
        DataEntry::GroupArray {
            path: TablePath::new(vec!["rows".to_string()]),
            items: vec![int(1), object(vec![("k", string("v"))])],
            position: P,
        },
        DataEntry::ObjectProperty {
            name: "settings".to_string(),
            data_type: None,
            object: Box::new(object(vec![("volume", int(11)), ("inner", object(vec![("deep", null())]))])),
            position: P,
        },
    ]);
    ast.enums = Some(local_enums());
    ast.security = Some(SecuritySection::new(
        vec![SecurityEntry {
            block_key: "encryption".to_string(),
            fields: vec![
                SecurityField { key: "algorithm".to_string(), value: string("aes256-gcm"), position: P },
                SecurityField { key: "salt".to_string(), value: prefixed("b", vec![string("00ff")]), position: P },
            ],
            position: P,
        }],
        P,
    ));
    ast
}

#[test]
fn unpack_document_reads_what_unpack_reads_over_the_corpus() {
    let mut unpacked = 0;
    for (label, document) in corpus_documents() {
        let packed = BinaryPacker::new().pack_document(&document);
        if !packed.is_success {
            continue;
        }
        unpacked += 1;
        let (old, new) = (owned_unpack(&packed.binary_data), document_unpack(&packed.binary_data));
        assert!(matches!(old, Unpacked::Done { .. }), "{label}: the owned unpacker should read its own file");
        assert_eq!(old, new, "{label}");
    }
    assert!(unpacked >= 55, "only {unpacked} corpus files were unpacked");
}

#[test]
fn unpack_document_puts_both_sections_in_one_arena() {
    // `@DATA` and `@SECURITY` are decoded into arenas of their own and then joined, so a
    // document with both has to resolve the ids of the section that was appended.
    let packed = BinaryPacker::new().pack(&all_sections_tree());
    assert!(packed.is_success, "{:?}", packed.errors);
    let result = BinaryUnpacker::new().unpack_document(&packed.binary_data);
    assert!(result.is_success, "{:?}", result.errors);
    let document = result.document.expect("a document");

    assert!(document.data.is_some() && document.security.is_some() && document.enums.is_some());
    assert_eq!(owned_unpack(&packed.binary_data), document_unpack(&packed.binary_data));

    let security = document.security.as_ref().unwrap();
    let field = &security.entries[0].fields[1];
    assert_eq!(field.key, "salt");
    assert_eq!(
        format!("{}", document.arena.value_ref(field.value)),
        format!("{}", prefixed("b", vec![string("00ff")]))
    );
}

#[test]
fn unpack_document_fails_the_way_unpack_fails_on_files_that_are_not_binaries() {
    let packed = BinaryPacker::new().pack(&all_sections_tree()).binary_data;
    let mut wrong_checksum = packed.clone();
    *wrong_checksum.last_mut().unwrap() ^= 0xFF;

    for (label, bytes) in [
        ("empty", Vec::new()),
        ("too short", vec![1, 2, 3]),
        ("a checksum that does not match", wrong_checksum),
        ("text", b"this is not a dixscript binary at all, it only has to be long enough".to_vec()),
    ] {
        let (old, new) = (owned_unpack(&bytes), document_unpack(&bytes));
        assert!(matches!(old, Unpacked::Failed(_)), "{label}: the owned unpacker should fail, got {old:?}");
        assert_eq!(old, new, "{label}");
    }
}

/// The payload of a packed file with the checksum computed again after `edit`: a changed file
/// that the checksum check lets through.
fn with_valid_checksum(binary: &[u8], edit: impl FnOnce(&mut Vec<u8>)) -> Vec<u8> {
    let mut payload = ChecksumValidator::validate_and_extract(binary).expect("the checksum holds");
    edit(&mut payload);
    ChecksumValidator::append_checksum(&payload)
}

/// Where a section starts and ends in the payload of a packed file.
fn section_range(binary: &[u8], wanted: &str) -> (usize, usize) {
    let data = ChecksumValidator::validate_and_extract(binary).expect("the checksum holds");
    let mut cursor = Cursor::new(data.as_slice());
    let header = BinaryHeader::read_from(&mut cursor).expect("the header reads");
    cursor.set_position(header.offset_table_position as u64);
    for _ in 0..header.section_count {
        let offset = SectionOffset::read_from(&mut cursor).expect("an offset entry reads");
        if format!("{:?}", offset.section_id) == wanted {
            return (offset.offset as usize, (offset.offset + offset.length) as usize);
        }
    }
    panic!("no {wanted} section");
}

#[test]
fn the_unpackers_agree_on_every_truncation_of_a_file_with_every_section() {
    // A truncation changes no count or length field, so it is safe to run on the original
    // reader too: it cannot ask for an absurd allocation. (Editing a length field can: the
    // bytes after it are then read as a count, and the section readers, which this slice does
    // not change, size a `Vec` from that count before reading anything.)
    let packed = BinaryPacker::new().pack(&all_sections_tree());
    assert!(packed.is_success, "{:?}", packed.errors);
    let payload_len = ChecksumValidator::validate_and_extract(&packed.binary_data).unwrap().len();

    let (mut failures, mut decoded) = (0, 0);
    for length in 0..payload_len {
        let bytes = with_valid_checksum(&packed.binary_data, |payload| payload.truncate(length));
        let (old, new) = (owned_unpack(&bytes), document_unpack(&bytes));
        assert_eq!(old, new, "truncated to {length} bytes");
        if matches!(old, Unpacked::Done { .. }) { decoded += 1 } else { failures += 1 }
    }
    // The offset table is the last thing in the payload, so every truncation cuts into it or
    // into a section it names, and every one of them has to fail.
    assert!(payload_len > 500, "the file is only {payload_len} bytes");
    assert_eq!(decoded, 0, "{decoded} truncations still decoded");
    assert_eq!(failures, payload_len, "not every truncation was compared");
}

#[test]
fn the_unpackers_fail_the_same_way_on_malformed_sections() {
    let packed = BinaryPacker::new().pack(&all_sections_tree());
    assert!(packed.is_success, "{:?}", packed.errors);
    let (data_start, _) = section_range(&packed.binary_data, "Data");
    let (security_start, _) = section_range(&packed.binary_data, "Security");

    // [id: 4][length: 4][entry count: 4][entry type: 1] ...
    let cases: Vec<(&str, usize, u8)> = vec![
        ("an unknown data entry type", data_start + 12, 0x09),
        ("a data entry type of zero", data_start + 12, 0x00),
        ("a data section with the wrong id", data_start, 0xFF),
        ("a security section with the wrong id", security_start, 0xFF),
        ("a data section with no entries", data_start + 8, 0x00),
        ("a security section with no entries", security_start + 8, 0x00),
    ];
    for (label, position, value) in cases {
        let bytes = with_valid_checksum(&packed.binary_data, |payload| payload[position] = value);
        let (old, new) = (owned_unpack(&bytes), document_unpack(&bytes));
        assert_eq!(old, new, "{label}");
    }

    // The first data entry is `integer = -7`: its type byte, then the name, then the value.
    // Setting the value's type tag to one that is not a tag at all, to Null, and to a tag whose
    // payload is longer than what follows makes the decoder fail, succeed and run out of
    // bytes, in that order.
    let simple_value_tag = data_start + 12 + 1 + 4 + "integer".len();
    for tag in [0x7Eu8, ValueTypeTag::Null as u8, ValueTypeTag::Regex as u8, ValueTypeTag::Invalid as u8] {
        let bytes = with_valid_checksum(&packed.binary_data, |payload| payload[simple_value_tag] = tag);
        let (old, new) = (owned_unpack(&bytes), document_unpack(&bytes));
        assert_eq!(old, new, "value tag {tag:#04x}");
    }
}

#[test]
fn an_object_property_that_decodes_to_another_type_fails_the_same_way() {
    // A one-entry tree whose only data entry is an object property with an empty object; the
    // value is `[Object tag][count: 4 bytes]` at the end of the data section.
    let mut ast = tree(vec![DataEntry::ObjectProperty {
        name: "o".to_string(),
        data_type: None,
        object: Box::new(object(vec![])),
        position: P,
    }]);
    ast.config = None;
    let packed = BinaryPacker::new().pack(&ast);
    assert!(packed.is_success, "{:?}", packed.errors);
    let (_, data_end) = section_range(&packed.binary_data, "Data");

    let intact = owned_unpack(&packed.binary_data);
    assert!(matches!(intact, Unpacked::Done { .. }), "{intact:?}");
    assert_eq!(intact, document_unpack(&packed.binary_data));

    let bytes = with_valid_checksum(&packed.binary_data, |payload| payload[data_end - 5] = ValueTypeTag::Null as u8);
    let (old, new) = (owned_unpack(&bytes), document_unpack(&bytes));
    match &old {
        Unpacked::Failed(errors) => assert!(errors.iter().any(|e| e.contains("Expected object literal for 'o'")), "{errors:?}"),
        other => panic!("expected a failure, got {other:?}"),
    }
    assert_eq!(old, new);
}

// ---------------------------------------------------------------------------------------------
// The value decoder
// ---------------------------------------------------------------------------------------------

/// What a value decoder did with some bytes.
#[derive(Debug, PartialEq)]
enum Decoded {
    Panicked,
    Failed { message: String, kind: std::io::ErrorKind, consumed: u64 },
    Done { value: String, consumed: u64, total_values: usize, value_counts: BTreeMap<String, usize> },
}

fn owned_decode(bytes: &[u8]) -> Decoded {
    std::panic::catch_unwind(|| {
        let mut ctx = BinarySerializationContext::new();
        let mut cursor = Cursor::new(bytes);
        match ValueDecoder::new().decode_value(&mut cursor, &mut ctx) {
            Ok(value) => Decoded::Done {
                value: format!("{value:?}"),
                consumed: cursor.position(),
                total_values: ctx.statistics.total_values,
                value_counts: ctx.statistics.value_counts.into_iter().collect(),
            },
            Err(e) => Decoded::Failed { message: e.to_string(), kind: e.kind(), consumed: cursor.position() },
        }
    })
    .unwrap_or(Decoded::Panicked)
}

fn arena_decode(bytes: &[u8]) -> Decoded {
    std::panic::catch_unwind(|| {
        let mut ctx = BinarySerializationContext::new();
        let mut arena = Arena::new();
        let mut cursor = Cursor::new(bytes);
        match ValueDecoder::new().decode_node(&mut cursor, &mut arena, &mut ctx) {
            Ok(id) => Decoded::Done {
                value: format!("{:?}", arena.export_value(id)),
                consumed: cursor.position(),
                total_values: ctx.statistics.total_values,
                value_counts: ctx.statistics.value_counts.into_iter().collect(),
            },
            Err(e) => Decoded::Failed { message: e.to_string(), kind: e.kind(), consumed: cursor.position() },
        }
    })
    .unwrap_or(Decoded::Panicked)
}

/// The encodings of one value of every type, nested ones included, as the packer writes them.
fn encoded_values() -> Vec<(&'static str, Vec<u8>)> {
    let values: Vec<(&'static str, Value)> = vec![
        ("integer", int(-7)),
        ("long", Value::Long { value: -(1 << 40), position: P }),
        ("float", Value::Float { value: 1.5, position: P }),
        ("double", Value::Double { value: -2.25, position: P }),
        ("string", string("héllo")),
        ("boolean", Value::Boolean { value: true, position: P }),
        ("null", null()),
        ("hex", Value::HexColor { value: "#FF00AA80".to_string(), position: P }),
        ("date", Value::Date { value: "2024-02-29".to_string(), position: P }),
        ("timestamp", Value::Timestamp { value: "2024-02-29T10:20:30Z".to_string(), position: P }),
        ("array", array(vec![int(1), string("two"), null()])),
        ("nested arrays", arrays_nested_to(4)),
        ("object", object(vec![("a", int(1)), ("bee", object(vec![("c", string("d"))])), ("e", array(vec![int(5)]))])),
        ("tuple", prefixed("t", vec![int(1), string("a"), Value::Boolean { value: false, position: P }])),
        ("blob", prefixed("b", vec![string("deadBEEF")])),
        ("regex", prefixed("r", vec![string("^[a-z]+$")])),
        ("enum", Value::EnumValue { enum_name: "Color".to_string(), value: "Green".to_string(), position: P }),
    ];
    values
        .into_iter()
        .map(|(label, value)| {
            let mut bytes = Vec::new();
            let mut ctx = BinarySerializationContext::new();
            ValueEncoder::new()
                .with_enums(ValueEncoder::build_local_enums(Some(&local_enums())))
                .encode_value(&mut bytes, &value, &mut ctx)
                .unwrap_or_else(|e| panic!("{label} does not encode: {e}"));
            (label, bytes)
        })
        .collect()
}

#[test]
fn decode_node_reads_every_value_type_the_way_decode_value_does() {
    for (label, bytes) in encoded_values() {
        let (old, new) = (owned_decode(&bytes), arena_decode(&bytes));
        assert!(matches!(old, Decoded::Done { .. }), "{label}: {old:?}");
        assert_eq!(old, new, "{label}");
    }
}

#[test]
fn the_value_decoders_agree_on_every_edit_and_truncation_of_every_value_type() {
    // Every count and length the value decoder reads is checked against a limit before
    // anything is allocated, except an object key's length, which can reach 2 GB of address
    // space and no more, so editing any byte here is safe on the original decoder too.
    let (mut compared, mut failures, mut panics) = (0usize, 0usize, 0usize);
    for (label, bytes) in encoded_values() {
        let mut check = |what: String, edited: Vec<u8>| {
            let (old, new) = (owned_decode(&edited), arena_decode(&edited));
            assert_eq!(old, new, "{label}: {what}");
            compared += 1;
            match old {
                Decoded::Panicked => panics += 1,
                Decoded::Failed { .. } => failures += 1,
                Decoded::Done { .. } => {}
            }
        };
        for position in 0..bytes.len() {
            for replacement in [0x00u8, 0x01, 0x02, 0x05, 0x06, 0x07, 0x7F, 0x80, 0xFE, 0xFF] {
                let mut edited = bytes.clone();
                edited[position] = replacement;
                check(format!("byte {position} set to {replacement:#04x}"), edited);
            }
        }
        for length in 0..bytes.len() {
            check(format!("truncated to {length} bytes"), bytes[..length].to_vec());
        }
    }
    assert!(compared > 2_000, "only {compared} edits were compared");
    assert!(failures > 1_000, "only {failures} of the edits made the decoder fail");
    assert!(panics > 0, "no edit reached the decoder's capacity-overflow panic");
    assert!(compared - failures - panics > 300, "too few edits still decoded");
}

// ---------------------------------------------------------------------------------------------
// The loader
// ---------------------------------------------------------------------------------------------

/// `compile_with_dlm_from_str` for a source with no DLM modules (a source with a compressor or
/// an encryptor returns an encrypted container with a random nonce, which cannot be compared
/// byte for byte; the DLM round-trip tests cover that branch) packs the document directly
/// and adds the default security section to it. The flow it replaced compiled to an owned tree,
/// cloned it, put the default security section on the clone and packed that; rebuilt here from
/// the owned entry points, it is the oracle.
///
/// Both flows compile the source themselves, and a few corpus files compile to a different
/// byte sequence every time (a quickfunction that returns an object lists its properties in
/// hash order). A file is compared only if the old flow agrees with itself across two compiles.
#[test]
fn compile_with_dlm_from_str_packs_what_the_clone_and_pack_flow_packed() {
    let loader = DixLoader::new_silent();

    // @CONFIG carries the time of the compile, so it is left out of every comparison.
    let without_config = |binary: &[u8]| -> Vec<(String, Vec<u8>)> {
        sections(binary).into_iter().filter(|(id, _)| id != "Config").collect()
    };
    let old_flow = |source: &str, label: &str| {
        let ast = loader.compile_to_resolved_ast_from_str(source, label).expect("compiles");
        let mut with_security = ast.clone();
        with_security.security = Some(SecurityUtilities::ensure_valid_security_section(
            with_security.security,
            with_security.dlm.as_ref(),
        ));
        let packed = BinaryPacker::new().pack(&with_security);
        assert!(packed.is_success, "{label}: {:?}", packed.errors);
        without_config(&packed.binary_data)
    };

    let (mut compared, mut skipped) = (0, 0);
    for relative in [
        "tests/cli/01_basic.mdix",
        "tests/cli/02_enums.mdix",
        "tests/cli/03_quickfuncs.mdix",
        "tests/cli/04_all_types.mdix",
        "tests/cli/05_game_config.mdix",
    ] {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../mdix_files").join(relative);
        let source = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{relative}: {e}"));
        let label = path.to_string_lossy().into_owned();

        let expected = old_flow(&source, &label);
        if expected != old_flow(&source, &label) {
            skipped += 1;
            continue;
        }

        let actual = loader.compile_with_dlm_from_str(&source, &label).expect("compiles with DLM");
        assert!(actual.is_success, "{relative}: {:?}", actual.errors);
        let actual = without_config(&actual.processed_data);

        let ids = |sections: &[(String, Vec<u8>)]| sections.iter().map(|(id, _)| id.clone()).collect::<Vec<_>>();
        assert!(ids(&actual).contains(&"Security".to_string()), "{relative}: no security section");
        assert_eq!(ids(&expected), ids(&actual), "{relative}: the sections present");
        for ((id, expected_bytes), (_, actual_bytes)) in expected.iter().zip(&actual) {
            assert_eq!(expected_bytes, actual_bytes, "{relative}: the {id} section");
        }
        compared += 1;
    }
    assert!(compared >= 3, "only {compared} files were compared ({skipped} skipped)");
    assert!(skipped <= 2, "{skipped} files compile differently each time");
}

#[test]
fn a_table_path_with_an_empty_segment_reads_the_same() {
    // The writer joins the segments with dots, so an empty one gives "a..b"; both readers
    // drop the empty segment when they split the path again.
    let mut ast = tree(vec![
        DataEntry::TableProperty {
            path: TablePath::new(vec!["a".to_string(), String::new(), "b".to_string()]),
            properties: vec![PropertyAssignment { name: "p".to_string(), data_type: None, value: int(1), position: P }],
            position: P,
        },
        DataEntry::GroupArray {
            path: TablePath::new(vec![String::new(), "g".to_string()]),
            items: vec![int(2)],
            position: P,
        },
    ]);
    ast.config = None;
    let packed = BinaryPacker::new().pack(&ast);
    assert!(packed.is_success, "{:?}", packed.errors);
    let (old, new) = (owned_unpack(&packed.binary_data), document_unpack(&packed.binary_data));
    assert!(matches!(old, Unpacked::Done { .. }), "{old:?}");
    assert_eq!(old, new);
}

/// `levels` containers inside each other, innermost holding one null, written byte by byte:
/// the encoder refuses to write more than `MAX_NESTING_DEPTH` levels, the decoders have to.
fn raw_nesting(container: ValueTypeTag, levels: usize) -> Vec<u8> {
    let mut bytes = Vec::new();
    for _ in 0..levels {
        match container {
            ValueTypeTag::Array => {
                bytes.push(ValueTypeTag::Array as u8);
                bytes.extend_from_slice(&1i32.to_le_bytes());
                bytes.push(ValueTypeTag::Null as u8);
            }
            ValueTypeTag::Object => {
                bytes.push(ValueTypeTag::Object as u8);
                bytes.extend_from_slice(&1i32.to_le_bytes());
                bytes.extend_from_slice(&1i32.to_le_bytes());
                bytes.push(b'k');
            }
            ValueTypeTag::Tuple => {
                bytes.push(ValueTypeTag::Tuple as u8);
                bytes.push(1);
            }
            other => panic!("{other:?} does not nest"),
        }
    }
    bytes.push(ValueTypeTag::Null as u8);
    bytes
}

#[test]
fn the_value_decoders_agree_on_nesting_at_and_past_the_limit() {
    for container in [ValueTypeTag::Array, ValueTypeTag::Object, ValueTypeTag::Tuple] {
        for levels in [MAX_NESTING_DEPTH - 1, MAX_NESTING_DEPTH, MAX_NESTING_DEPTH + 1, MAX_NESTING_DEPTH + 3] {
            let bytes = raw_nesting(container, levels);
            let (old, new) = (owned_decode(&bytes), arena_decode(&bytes));
            assert_eq!(old, new, "{container:?} nested {levels} deep");
            match (levels <= MAX_NESTING_DEPTH, &old) {
                (true, Decoded::Done { .. }) | (false, Decoded::Failed { .. }) => {}
                _ => panic!("{container:?} nested {levels} deep: unexpected {old:?}"),
            }
        }
    }
}

#[test]
fn the_value_decoders_agree_on_counts_at_and_past_their_limits() {
    let mut cases: Vec<(String, Vec<u8>)> = Vec::new();
    for count in [0i32, 1, MAX_ARRAY_LENGTH as i32, MAX_ARRAY_LENGTH as i32 + 1, -1] {
        let mut bytes = vec![ValueTypeTag::Array as u8];
        bytes.extend_from_slice(&count.to_le_bytes());
        bytes.push(ValueTypeTag::Null as u8);
        cases.push((format!("array of {count}"), bytes));
    }
    for count in [0i32, 1, MAX_OBJECT_PROPERTIES as i32, MAX_OBJECT_PROPERTIES as i32 + 1, -1] {
        let mut bytes = vec![ValueTypeTag::Object as u8];
        bytes.extend_from_slice(&count.to_le_bytes());
        cases.push((format!("object of {count}"), bytes));
    }
    for length in [0i32, 1, MAX_STRING_LENGTH as i32, MAX_STRING_LENGTH as i32 + 1, -1] {
        for tag in [ValueTypeTag::String, ValueTypeTag::Regex] {
            let mut bytes = vec![tag as u8];
            bytes.extend_from_slice(&length.to_le_bytes());
            cases.push((format!("{tag:?} of length {length}"), bytes));
        }
        let mut blob = vec![ValueTypeTag::Blob as u8, 0];
        blob.extend_from_slice(&length.to_le_bytes());
        cases.push((format!("blob of length {length}"), blob));
    }
    for (label, bytes) in cases {
        assert_eq!(owned_decode(&bytes), arena_decode(&bytes), "{label}");
    }
}
