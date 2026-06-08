//! Runs a .mdix file through the compilation pipeline and BinaryPacker,
//! writing raw binary output plus a hex dump and base64 string.
//!
//! Usage: cargo run --example binary_capture -- <input.mdix> <output_dir>

use std::{env, fs, path::Path};

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use dixscript::Compiler::Core::{
    BinarySerialization::BinaryPacker,
    Config::{ConfigSectionHandler, OperationalSettings},
    GeneralAstEnhancer, GeneralParser, GeneralSemanticAnalyzer,
    Tokenizer::{split_config_tokens, Tokenizer},
};

fn main() {
    let args: Vec<String> = env::args().collect();
    let input_path = args
        .get(1)
        .map(String::as_str)
        .unwrap_or("mdix_files/tests/dlm/serialize_target.mdix");
    let output_dir = args.get(2).map(String::as_str).unwrap_or("binary-output");

    let source = fs::read_to_string(input_path)
        .unwrap_or_else(|e| panic!("cannot read {}: {}", input_path, e));

    // Stage 1: tokenize full source with minimal initial settings
    let initial    = OperationalSettings::default();
    let tok_result = Tokenizer::new(&source, &initial).tokenize();

    // Stage 2: split @CONFIG tokens from the rest of the stream
    let split = split_config_tokens(tok_result.tokens);

    // Stage 3: process @CONFIG to derive operational settings
    let mut config_handler = ConfigSectionHandler::new(None);
    let config_result      = config_handler.process_config_tokens(&split.config_tokens);

    let mut settings = config_result.operational_settings;
    settings.source_file_path = Some(input_path.to_string());

    // Stage 4: parse the rest of the token stream
    let parser = GeneralParser::new(split.rest_tokens, &config_result.config_section, &settings)
        .unwrap_or_else(|e| panic!("parser init: {}", e.message()));
    let ast = parser.parse().unwrap_or_else(|e| panic!("parse: {}", e.message()));

    // Stage 5: semantic analysis
    let sem_result = GeneralSemanticAnalyzer::new(&ast, &settings).analyze();
    if !sem_result.is_success {
        eprintln!(
            "semantic warnings: {:?}",
            sem_result.errors.iter().map(|e| &e.message).collect::<Vec<_>>()
        );
    }

    // Stage 6: AST enhancement
    let enh_result = GeneralAstEnhancer::new(&settings).enhance(&ast, Some(&sem_result));

    // Stage 7: binary pack
    let mut packer    = BinaryPacker::new();
    let pack_result   = packer.pack(&enh_result.enhanced_ast);

    if !pack_result.is_success {
        eprintln!("pack errors: {:?}", pack_result.errors);
        std::process::exit(1);
    }

    let binary = &pack_result.binary_data;

    fs::create_dir_all(output_dir).unwrap();
    let stem = Path::new(input_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output");

    fs::write(format!("{}/{}.bin", output_dir, stem), binary).unwrap();

    let b64 = BASE64.encode(binary);
    fs::write(format!("{}/{}.b64", output_dir, stem), &b64).unwrap();

    let hex = build_hex_dump(binary);
    fs::write(format!("{}/{}.hex", output_dir, stem), &hex).unwrap();

    let meta = format!(
        r#"{{"input":"{input_path}","size_bytes":{size},"sections":{sections},"compression_ratio":{ratio:.4}}}"#,
        input_path = input_path,
        size       = binary.len(),
        sections   = pack_result.statistics.total_sections,
        ratio      = pack_result.compression_ratio,
    );
    fs::write(format!("{}/{}.json", output_dir, stem), &meta).unwrap();

    println!("BINARY_SIZE:{}", binary.len());
    println!("BINARY_SECTIONS:{}", pack_result.statistics.total_sections);
    println!("BINARY_B64:{}", b64);
    println!("OUTPUT_STEM:{}/{}", output_dir, stem);

    eprintln!(
        "binary_capture: {} bytes, {} sections → {}/{}.bin",
        binary.len(),
        pack_result.statistics.total_sections,
        output_dir,
        stem
    );
}

fn build_hex_dump(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len() * 4);
    for (i, chunk) in data.chunks(16).enumerate() {
        out.push_str(&format!("{:08x}  ", i * 16));
        for (j, byte) in chunk.iter().enumerate() {
            if j == 8 { out.push(' '); }
            out.push_str(&format!("{:02x} ", byte));
        }
        let pad = 16 - chunk.len();
        for j in 0..pad {
            if chunk.len() + j == 8 { out.push(' '); }
            out.push_str("   ");
        }
        out.push_str(" |");
        for byte in chunk {
            out.push(if *byte >= 0x20 && *byte < 0x7f { *byte as char } else { '.' });
        }
        out.push_str("|\n");
    }
    out
}
