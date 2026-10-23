// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/compiler.md, section "tests/security_analyzer_document.rs"
// ============================================================================
//! Tests for the `@SECURITY` analyzer reading a `Document`.
//!
//! `GeneralSemanticAnalyzer` can be given the arena form of a file with `with_document`, and the
//! `@SECURITY` analyzer then reads the `Document` where it used to read the owned tree. It must
//! report exactly what it reported before. The tests check three things:
//!
//! * the two paths, and the two analyzer entry points, give identical diagnostics, positions
//!   and section results for every source under all three error strategies;
//! * the diagnostics are the ones the owned-tree analyzer recorded before the change (the
//!   `RECORDING` table, produced by running the original code), including the way a repeated
//!   key or block is resolved;
//! * the analyzer really reads the `Document` when it has one and the owned tree when it does
//!   not. A pair whose two halves disagree shows which half was read.
//!
//! The parser adds a default `@SECURITY` section when `@DLM` asks for an encryptor, so the
//! "section required" error (`SEM0002`) can only be reached through such a disagreeing pair.

use dixscript::Compiler::Core::SectionAnalyzers::SecuritySectionAnalyzer;

use dixscript::Compiler::AST::arena::Document;
use dixscript::Compiler::AST::data_types::{CompatibilityMode, ErrorHandlingStrategy};
use dixscript::Compiler::AST::{DixScript, Position};
use dixscript::Compiler::Core::Config::{ConfigSectionHandler, OperationalSettings};
use dixscript::Compiler::Core::Tokenizer::{split_config_tokens, Tokenizer};
use dixscript::Compiler::Core::{
    GeneralParser, GeneralSemanticAnalyzer, SectionAnalysisResult, SemanticAnalysisResult,
};
use dixscript::Compiler::Utilities::SymbolTable;
use dixscript::ErrorManager::ErrorManager;

/// One parsed file: the document, its export, and the settings the parse produced.
struct Parsed {
    document: Document,
    ast: DixScript,
    settings: OperationalSettings,
}

/// Runs the loader's first four stages and returns the document, or `None` when the parse
/// fails.
fn parse(source: &str, strategy: ErrorHandlingStrategy) -> Option<Parsed> {
    let label = "security-analyzer-document-test";
    let em = ErrorManager::new_isolated_silent();
    let initial = OperationalSettings {
        source_file_path: Some(label.to_string()),
        compatibility_mode: CompatibilityMode::Strict,
        ..OperationalSettings::default()
    };
    let tokens =
        Tokenizer::new_with_error_manager_from_bytes(source.as_bytes(), &initial, em.clone()).tokenize();
    let split = split_config_tokens(tokens.tokens);
    let mut handler = ConfigSectionHandler::new_with_error_manager(None, em.clone());
    let config = handler.process_config_tokens(&split.config_tokens);
    let mut settings = config.operational_settings;
    settings.source_file_path = Some(label.to_string());
    settings.error_handling_strategy = strategy;
    em.update_settings(settings.clone());
    let document = GeneralParser::new_for_lsp(split.rest_tokens, &config.config_section, &settings, em)
        .expect("the parser starts")
        .parse_document()
        .ok()?;
    let ast = document.export();
    Some(Parsed { document, ast, settings })
}

/// Analyzes with a fresh error manager and returns the result and what reached the manager.
fn analyze(
    ast: &DixScript,
    settings: &OperationalSettings,
    document: Option<&Document>,
) -> (SemanticAnalysisResult, Vec<String>) {
    let em = ErrorManager::new_isolated_silent();
    let analyzer = GeneralSemanticAnalyzer::new_with_error_manager(ast, settings, em.clone());
    let result = match document {
        Some(doc) => analyzer.with_document(doc).analyze(),
        None => analyzer.analyze(),
    };
    let mut logged: Vec<String> = em
        .get_semantic_errors()
        .iter()
        .map(|e| format!("{}:{}:{}:{:?}:{:?}", e.line, e.column, e.message, e.section_name, e.suggestion))
        .collect();
    logged.sort();
    (result, logged)
}

/// Everything about a result that the tests compare, in a stable order.
fn describe(result: &SemanticAnalysisResult, logged: &[String]) -> String {
    let mut errors: Vec<String> = result
        .errors
        .iter()
        .map(|e| format!("[{}] {} | {} | {} | {} | {:?}", e.error_id, e.error_type, e.section_name, e.message, e.suggestion, e.position))
        .collect();
    errors.sort();
    let mut warnings: Vec<String> = result
        .warnings
        .iter()
        .map(|w| format!("[{}] {} | {} | {:?}", w.warning_id, w.message, w.section_name, w.position))
        .collect();
    warnings.sort();
    let mut sections: Vec<String> = result
        .section_results
        .iter()
        .map(|(name, s)| format!("{name} {}", describe_section(s)))
        .collect();
    sections.sort();
    format!(
        "success={}\nerrors={errors:#?}\nwarnings={warnings:#?}\nsections={sections:#?}\nlogged={logged:#?}",
        result.is_success
    )
}

/// One section result in a stable order.
fn describe_section(s: &SectionAnalysisResult) -> String {
    let mut e: Vec<String> = s.errors.iter().map(|e| format!("{}:{}:{:?}:{}", e.error_id, e.message, e.position, e.suggestion)).collect();
    e.sort();
    let mut w: Vec<String> = s.warnings.iter().map(|w| format!("{}:{}:{:?}", w.warning_id, w.message, w.position)).collect();
    w.sort();
    format!("ok={} errors={e:?} warnings={w:?}", s.is_success)
}

fn position(p: Option<Position>) -> String {
    match p {
        None => "-".to_string(),
        Some(p) => format!("{}:{}", p.line, p.column),
    }
}

/// The diagnostics of one section as `E|id|message|line:col` and `W|id|message|line:col`
/// lines, sorted.
fn section_lines(result: &SemanticAnalysisResult, section: &str) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for e in result.errors.iter().filter(|e| e.section_name == section) {
        lines.push(format!("E|{}|{}|{}", e.error_id, e.message, position(e.position)));
    }
    for w in result.warnings.iter().filter(|w| w.section_name == section) {
        lines.push(format!("W|{}|{}|{}", w.warning_id, w.message, position(w.position)));
    }
    lines.sort();
    lines
}

fn source(name: &str) -> &'static str {
    SOURCES.iter().find(|(n, _)| *n == name).unwrap_or_else(|| panic!("unknown source {name}")).1
}

fn strategy(name: &str) -> ErrorHandlingStrategy {
    match name {
        "halt" => ErrorHandlingStrategy::Halt,
        "continue" => ErrorHandlingStrategy::Continue,
        "recover" => ErrorHandlingStrategy::Recover,
        other => panic!("unknown strategy {other}"),
    }
}

const STRATEGIES: [&str; 3] = ["halt", "continue", "recover"];

/// The recorded cases: name, strategy, overall success, and the recorded section lines.
fn recorded_cases() -> Vec<(String, String, bool, Vec<String>)> {
    let mut cases: Vec<(String, String, bool, Vec<String>)> = Vec::new();
    for line in RECORDING.lines().filter(|l| !l.is_empty()) {
        if let Some(header) = line.strip_prefix('@') {
            let mut parts = header.split('|');
            let name = parts.next().expect("name").to_string();
            let strategy = parts.next().expect("strategy").to_string();
            let success = parts.next().expect("success") == "success=true";
            cases.push((name, strategy, success, Vec::new()));
        } else {
            cases.last_mut().expect("a header comes first").3.push(line.to_string());
        }
    }
    cases
}

/// The sources the tests run, as `(name, text)`. Each exercises a different check.
const SOURCES: &[(&str, &str)] = &[
    ("sec_empty", r##"@SECURITY(
)
"##),
    ("sec_keyfile_ok", r##"@SECURITY(
  encryption -> { mode = "keyfile", algorithm = "aes256-gcm" }
  keystore -> { auto_generate = true, backup_count = 3 }
)
"##),
    ("sec_password_ok", r##"@SECURITY(
  encryption -> { mode = "password", algorithm = "chacha20-poly1305", kdf = "argon2id", kdf_memory = 65536, kdf_iterations = 3, kdf_parallelism = 4 }
)
"##),
    ("sec_password_low_params", r##"@SECURITY(
  encryption -> { mode = "password", algorithm = "aes256", kdf = "pbkdf2", kdf_memory = 1000, kdf_iterations = 1, kdf_parallelism = 1 }
)
"##),
    ("sec_password_param_types", r##"@SECURITY(
  encryption -> { mode = "password", algorithm = "aes128", kdf = "argon2id", kdf_memory = 5000000000L, kdf_iterations = "three", kdf_parallelism = 2.5 }
)
"##),
    ("sec_password_unknown_kdf", r##"@SECURITY(
  encryption -> { mode = "password", algorithm = "aes128-gcm", kdf = "scrypt" }
)
"##),
    ("sec_password_no_kdf", r##"@SECURITY(
  encryption -> { mode = "password", algorithm = "aes128-gcm" }
)
"##),
    ("sec_manual_accepted", r##"@SECURITY(
  encryption -> { mode = "manual", algorithm = "xor", key = "k", iv = "i" }
  override -> { manual_key_warning_accepted = true }
)
"##),
    ("sec_manual_not_accepted", r##"@SECURITY(
  encryption -> { mode = "manual", key = "k", iv = "i" }
  override -> { manual_key_warning_accepted = false }
)
"##),
    ("sec_manual_accept_wrong_type", r##"@SECURITY(
  encryption -> { mode = "manual", key = "k", iv = "i" }
  override -> { manual_key_warning_accepted = "yes" }
)
"##),
    ("sec_manual_no_override", r##"@SECURITY(
  encryption -> { mode = "manual", key = "k" }
)
"##),
    ("sec_manual_override_repeated", r##"@SECURITY(
  encryption -> { mode = "manual", key = "k", iv = "i" }
  override -> { manual_key_warning_accepted = true, manual_key_warning_accepted = false }
)
"##),
    ("sec_xor", r##"@SECURITY(
  encryption -> { mode = "keyfile", algorithm = "xor" }
)
"##),
    ("sec_unknown_algorithm", r##"@SECURITY(
  encryption -> { mode = "keyfile", algorithm = "rot13" }
)
"##),
    ("sec_algorithm_wrong_type", r##"@SECURITY(
  encryption -> { mode = "keyfile", algorithm = 256 }
)
"##),
    ("sec_no_algorithm", r##"@SECURITY(
  encryption -> { mode = "keyfile" }
)
"##),
    ("sec_mode_missing", r##"@SECURITY(
  encryption -> { algorithm = "aes256-gcm" }
)
"##),
    ("sec_mode_wrong_type", r##"@SECURITY(
  encryption -> { mode = 5, algorithm = "aes256-gcm" }
)
"##),
    ("sec_mode_unknown", r##"@SECURITY(
  encryption -> { mode = "telepathy", algorithm = "aes256-gcm" }
)
"##),
    ("sec_no_encryption_block", r##"@SECURITY(
  keystore -> { backup_count = 2 }
)
"##),
    ("sec_empty_block", r##"@SECURITY(
  encryption -> { }
)
"##),
    ("sec_unknown_block", r##"@SECURITY(
  encryption -> { mode = "keyfile", algorithm = "aes256" }
  mystery -> { a = 1 }
)
"##),
    ("sec_case_insensitive_keys", r##"@SECURITY(
  ENCRYPTION -> { mode = "keyfile", algorithm = "aes256" }
  Keystore -> { backup_count = 99 }
  VALIDATION -> { checksum_algorithm = "SHA256" }
)
"##),
    ("sec_backup_range", r##"@SECURITY(
  encryption -> { mode = "keyfile", algorithm = "aes256" }
  keystore -> { backup_count = 11 }
)
"##),
    ("sec_backup_negative", r##"@SECURITY(
  encryption -> { mode = "keyfile", algorithm = "aes256" }
  keystore -> { backup_count = -1 }
)
"##),
    ("sec_backup_edges", r##"@SECURITY(
  encryption -> { mode = "keyfile", algorithm = "aes256" }
  keystore -> { backup_count = 10 }
)
"##),
    ("sec_backup_wrong_type", r##"@SECURITY(
  encryption -> { mode = "keyfile", algorithm = "aes256" }
  keystore -> { backup_count = "many", auto_generate = "yes" }
)
"##),
    ("sec_unknown_checksum", r##"@SECURITY(
  encryption -> { mode = "keyfile", algorithm = "aes256" }
  validation -> { checksum_algorithm = "md5" }
)
"##),
    ("sec_good_checksum", r##"@SECURITY(
  encryption -> { mode = "keyfile", algorithm = "aes256" }
  validation -> { checksum_algorithm = "hmac-sha512" }
)
"##),
    ("sec_checksum_wrong_type", r##"@SECURITY(
  encryption -> { mode = "keyfile", algorithm = "aes256" }
  validation -> { checksum_algorithm = 5 }
)
"##),
    ("sec_duplicate_encryption_blocks", r##"@SECURITY(
  encryption -> { mode = "password", algorithm = "xor" }
  encryption -> { mode = "keyfile", algorithm = "aes256-gcm" }
)
"##),
    ("sec_repeated_mode_key", r##"@SECURITY(
  encryption -> { mode = "password", mode = "keyfile", algorithm = "aes256" }
)
"##),
    ("sec_blocks_for_other_mode", r##"@SECURITY(
  encryption -> { mode = "keyfile", algorithm = "aes256" }
  override -> { manual_key_warning_accepted = true }
  keystore -> { backup_count = 50 }
)
"##),
    ("sec_dlm_requires_security", r##"@DLM(
  DEncryptor.aes256
)
@DATA(
  a = 1
)
"##),
    ("sec_dlm_with_security", r##"@DLM(
  DEncryptor.aes256
)
@SECURITY(
  encryption -> { mode = "keyfile", algorithm = "aes256-gcm" }
)
"##),
    ("sec_with_data_and_raw", r##"@DATA(
  a = 1
)
@SECURITY(
  encryption -> { mode = "keyfile", algorithm = "xor" }
)
@RAW(
  meta_data -> { id = "rs", format = "TXT" }
  content -> {
---rst
p
---rst
  }
)
"##),
    ("sec_long_kdf_params", r##"@SECURITY(
  encryption -> { mode = "password", algorithm = "aes256", kdf = "argon2id", kdf_memory = 100L, kdf_iterations = 1L, kdf_parallelism = 1 }
)
"##),
    ("sec_mode_password_only", r##"@SECURITY(
  encryption -> { mode = "password" }
)
"##),
];

/// The number of cases in `RECORDING`.
const EXPECTED_CASES: usize = 38;

/// What the owned-tree analyzer reported for each source before it moved to the arena,
/// captured by running the original code. A line `@name|strategy|success=..` starts a case; the
/// `E|id|message|line:col` and `W|id|message|line:col` lines after it are that section's
/// diagnostics (`-` for no position).
const RECORDING: &str = r##"
@sec_empty|continue|success=true
W|SEC_WARN001|SECURITY section is empty - default settings will be used|-
@sec_keyfile_ok|continue|success=true
@sec_password_ok|continue|success=true
@sec_password_low_params|continue|success=true
W|SEC_WARN005|KDF parameter 'kdf_iterations' value 1 below recommended minimum 3|-
W|SEC_WARN005|KDF parameter 'kdf_memory' value 1000 below recommended minimum 65536|-
W|SEC_WARN005|KDF parameter 'kdf_parallelism' value 1 below recommended minimum 4|-
@sec_password_param_types|continue|success=true
@sec_password_unknown_kdf|continue|success=true
W|SEC_WARN007|Unknown KDF algorithm: scrypt - will default to 'argon2id'|-
@sec_password_no_kdf|continue|success=true
W|SEC_WARN007|Key derivation function not specified - will default to 'argon2id'|-
W|SEC_WARN007|Required field 'kdf' missing for mode 'password' - will use default|-
@sec_manual_accepted|continue|success=true
W|SEC_WARN003|CRITICAL: Manual mode stores the encryption key in PLAINTEXT in source|-
W|SEC_WARN004|Algorithm 'xor' provides LOW security - obfuscation only|-
W|SEC_WARN006|Manual mode enabled - encryption key stored in PLAINTEXT|3:3
@sec_manual_not_accepted|continue|success=true
W|SEC_WARN003|CRITICAL: Manual mode stores the encryption key in PLAINTEXT in source|-
W|SEC_WARN006|Manual mode key warning not explicitly accepted - encryption may fail|3:3
W|SEC_WARN007|Encryption algorithm not specified - will default to 'aes256-gcm'|-
@sec_manual_accept_wrong_type|continue|success=true
W|SEC_WARN003|CRITICAL: Manual mode stores the encryption key in PLAINTEXT in source|-
W|SEC_WARN006|Manual mode key warning not explicitly accepted - encryption may fail|3:3
W|SEC_WARN007|Encryption algorithm not specified - will default to 'aes256-gcm'|-
@sec_manual_no_override|continue|success=true
W|SEC_WARN003|CRITICAL: Manual mode stores the encryption key in PLAINTEXT in source|-
W|SEC_WARN007|Encryption algorithm not specified - will default to 'aes256-gcm'|-
W|SEC_WARN007|Required field 'iv' missing for mode 'manual' - will use default|-
@sec_manual_override_repeated|continue|success=true
W|SEC_WARN003|CRITICAL: Manual mode stores the encryption key in PLAINTEXT in source|-
W|SEC_WARN006|Manual mode enabled - encryption key stored in PLAINTEXT|3:3
W|SEC_WARN007|Encryption algorithm not specified - will default to 'aes256-gcm'|-
@sec_xor|continue|success=true
W|SEC_WARN004|Algorithm 'xor' provides LOW security - obfuscation only|-
@sec_unknown_algorithm|continue|success=true
W|SEC_WARN007|Algorithm must be a string - will default to 'aes256-gcm'|2:37
@sec_algorithm_wrong_type|continue|success=true
W|SEC_WARN007|Algorithm must be a string - will default to 'aes256-gcm'|2:37
@sec_no_algorithm|continue|success=true
W|SEC_WARN007|Encryption algorithm not specified - will default to 'aes256-gcm'|-
W|SEC_WARN007|Required field 'algorithm' missing for mode 'keyfile' - will use default|-
@sec_mode_missing|continue|success=true
W|SEC_WARN007|Encryption mode not specified - defaulting to 'keyfile'|-
W|SEC_WARN007|Required field 'mode' missing for mode 'keyfile' - will use default|-
@sec_mode_wrong_type|continue|success=true
W|SEC_WARN007|Encryption mode must be a string - defaulting to 'keyfile'|2:19
@sec_mode_unknown|continue|success=true
W|SEC_WARN007|Encryption mode must be a string - defaulting to 'keyfile'|2:19
@sec_no_encryption_block|continue|success=true
W|SEC_WARN007|No encryption configuration found - defaults will be applied|-
@sec_empty_block|continue|success=true
W|SEC_WARN002|Security block 'encryption' is empty|2:3
W|SEC_WARN007|Encryption algorithm not specified - will default to 'aes256-gcm'|-
W|SEC_WARN007|Encryption mode not specified - defaulting to 'keyfile'|-
W|SEC_WARN007|Required field 'algorithm' missing for mode 'keyfile' - will use default|-
W|SEC_WARN007|Required field 'mode' missing for mode 'keyfile' - will use default|-
@sec_unknown_block|continue|success=true
W|SEC_WARN002|Unknown security block key: mystery (ignored)|3:3
@sec_case_insensitive_keys|continue|success=true
W|SEC_WARN008|Backup count 99 out of range (0-10) - will use default 3|3:17
@sec_backup_range|continue|success=true
W|SEC_WARN008|Backup count 11 out of range (0-10) - will use default 3|3:17
@sec_backup_negative|continue|success=true
W|SEC_WARN008|Backup count -1 out of range (0-10) - will use default 3|3:17
@sec_backup_edges|continue|success=true
@sec_backup_wrong_type|continue|success=true
@sec_unknown_checksum|continue|success=true
W|SEC_WARN007|Unknown checksum algorithm: md5 - will default to 'sha256'|3:19
@sec_good_checksum|continue|success=true
@sec_checksum_wrong_type|continue|success=true
@sec_duplicate_encryption_blocks|continue|success=true
@sec_repeated_mode_key|continue|success=true
@sec_blocks_for_other_mode|continue|success=true
W|SEC_WARN008|Backup count 50 out of range (0-10) - will use default 3|4:17
@sec_dlm_requires_security|continue|success=true
@sec_dlm_with_security|continue|success=true
@sec_with_data_and_raw|continue|success=true
W|SEC_WARN004|Algorithm 'xor' provides LOW security - obfuscation only|-
@sec_long_kdf_params|continue|success=true
W|SEC_WARN005|KDF parameter 'kdf_parallelism' value 1 below recommended minimum 4|-
@sec_mode_password_only|continue|success=true
W|SEC_WARN007|Encryption algorithm not specified - will default to 'aes256-gcm'|-
W|SEC_WARN007|Key derivation function not specified - will default to 'argon2id'|-
W|SEC_WARN007|Required field 'algorithm' missing for mode 'password' - will use default|-
W|SEC_WARN007|Required field 'kdf' missing for mode 'password' - will use default|-
"##;

// ==================== the two paths agree ====================

#[test]
fn document_path_and_owned_path_give_identical_diagnostics() {
    let mut compared = 0;
    for (name, text) in SOURCES {
        for strategy_name in STRATEGIES {
            let parsed = parse(text, strategy(strategy_name))
                .unwrap_or_else(|| panic!("{name} [{strategy_name}] should parse"));
            let (with_doc, logged_doc) = analyze(&parsed.ast, &parsed.settings, Some(&parsed.document));
            let (without, logged_owned) = analyze(&parsed.ast, &parsed.settings, None);
            assert_eq!(
                describe(&with_doc, &logged_doc),
                describe(&without, &logged_owned),
                "{name} [{strategy_name}]: the document path and the owned path disagree"
            );
            compared += 1;
        }
    }
    assert_eq!(compared, SOURCES.len() * 3);
}

#[test]
fn the_two_entry_points_agree() {
    let mut checked = 0;
    for (name, text) in SOURCES {
        let parsed = parse(text, ErrorHandlingStrategy::Continue).expect("parses");
        let (Some(owned), Some(section)) = (parsed.ast.security.as_ref(), parsed.document.security.as_ref()) else {
            continue;
        };
        let by_owned = SecuritySectionAnalyzer::new_with_error_manager(&parsed.settings, ErrorManager::new_isolated_silent())
            .analyze(owned, &mut SymbolTable::new());
        let by_doc = SecuritySectionAnalyzer::new_with_error_manager(&parsed.settings, ErrorManager::new_isolated_silent())
            .analyze_doc(section, &parsed.document.arena, &mut SymbolTable::new());
        assert_eq!(describe_section(&by_owned), describe_section(&by_doc), "{name}");
        checked += 1;
    }
    assert!(checked >= 35, "only {checked} sources have a @SECURITY section");
}

// ==================== the recorded behaviour ====================

#[test]
fn every_source_has_a_recorded_case() {
    let recorded = recorded_cases();
    assert_eq!(recorded.len(), EXPECTED_CASES);
    for (name, _) in SOURCES {
        assert!(recorded.iter().any(|(n, _, _, _)| n == name), "{name} has no recorded case");
    }
}

#[test]
fn security_diagnostics_match_the_recording() {
    for (name, strategy_name, success, mut expected) in recorded_cases() {
        let parsed = parse(source(&name), strategy(&strategy_name)).expect("parses");
        let (result, _) = analyze(&parsed.ast, &parsed.settings, Some(&parsed.document));
        expected.sort();
        assert_eq!(section_lines(&result, "SECURITY"), expected, "{name} [{strategy_name}]");
        assert_eq!(result.is_success, success, "{name} [{strategy_name}]: overall success");
    }
}

/// The security lines of one source under `continue`, read through the document.
fn lines_of(name: &str) -> Vec<String> {
    let parsed = parse(source(name), ErrorHandlingStrategy::Continue).expect("parses");
    let (result, _) = analyze(&parsed.ast, &parsed.settings, Some(&parsed.document));
    section_lines(&result, "SECURITY")
}

#[test]
fn the_last_of_a_repeated_key_decides_the_encryption_config() {
    // `mode = "password", mode = "keyfile"`: the last one wins here. If the first won, the mode
    // would be password and the missing `kdf` would be reported.
    assert_eq!(lines_of("sec_repeated_mode_key"), Vec::<String>::new());
}

#[test]
fn the_first_of_a_repeated_key_decides_manual_acceptance() {
    // `manual_key_warning_accepted = true, ... = false`: the manual-mode check takes the first
    // one, unlike the encryption config above. Taking the last would say the warning was not
    // accepted.
    let lines = lines_of("sec_manual_override_repeated");
    assert!(lines.contains(&"W|SEC_WARN006|Manual mode enabled - encryption key stored in PLAINTEXT|3:3".to_string()), "{lines:?}");
    assert!(!lines.iter().any(|l| l.contains("not explicitly accepted")), "{lines:?}");
}

#[test]
fn the_last_of_two_encryption_blocks_wins() {
    // The first block says `xor`, which would warn; the second is `aes256-gcm`.
    assert_eq!(lines_of("sec_duplicate_encryption_blocks"), Vec::<String>::new());
}

#[test]
fn kdf_parameters_must_be_integers_not_longs() {
    // `kdf_memory = 100L` and `kdf_iterations = 1L` are far below their minimums but are longs,
    // so they are ignored; only the integer `kdf_parallelism = 1` is reported.
    let lines = lines_of("sec_long_kdf_params");
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].starts_with("W|SEC_WARN005|KDF parameter 'kdf_parallelism'"), "{lines:?}");
}

#[test]
fn backup_count_range_is_inclusive_at_both_ends() {
    assert_eq!(lines_of("sec_backup_edges"), Vec::<String>::new(), "10 is in range");
    assert!(lines_of("sec_backup_range")[0].contains("Backup count 11 out of range"));
    assert!(lines_of("sec_backup_negative")[0].contains("Backup count -1 out of range"));
}

// ==================== which tree is read ====================

#[test]
fn the_analyzer_reads_the_document_when_one_is_attached() {
    // The document holds a section with warnings. The owned tree handed to the analyzer has no
    // @SECURITY section at all, so a warning can only come from the document.
    let parsed = parse(source("sec_xor"), ErrorHandlingStrategy::Continue).expect("parses");
    let empty = DixScript::new();

    let (with_doc, _) = analyze(&empty, &parsed.settings, Some(&parsed.document));
    assert_eq!(section_lines(&with_doc, "SECURITY").len(), 1);
    assert!(with_doc.section_results.contains_key("SECURITY"));

    let (without, _) = analyze(&empty, &parsed.settings, None);
    assert!(!without.section_results.contains_key("SECURITY"));
}

#[test]
fn the_analyzer_reads_the_owned_tree_when_no_document_is_attached() {
    // The mirror image: the document has no section and the owned tree has one.
    let parsed = parse(source("sec_xor"), ErrorHandlingStrategy::Continue).expect("parses");
    let empty_document = Document::from_owned(&DixScript::new());

    let (without, _) = analyze(&parsed.ast, &parsed.settings, None);
    assert_eq!(section_lines(&without, "SECURITY").len(), 1);

    let (with_empty, _) = analyze(&parsed.ast, &parsed.settings, Some(&empty_document));
    assert!(!with_empty.section_results.contains_key("SECURITY"));
}

#[test]
fn a_missing_section_is_looked_up_in_the_attached_document() {
    // `@DLM` asks for an encryptor (that section is read from the owned tree in both cases), so
    // a file with no @SECURITY section is an error. Which tree says "no section" decides it.
    let mut requires = parse(source("sec_dlm_requires_security"), ErrorHandlingStrategy::Continue).expect("parses");
    // The parser added a default @SECURITY section; take it away to model a file without one.
    requires.ast.security = None;
    let with_section = parse(source("sec_dlm_with_security"), ErrorHandlingStrategy::Continue).expect("parses");
    let without_section = parse("@DATA(\n  a = 1\n)\n", ErrorHandlingStrategy::Continue).expect("parses");

    // The tree has no section but the document does: no error, and the section is analyzed.
    let (result, _) = analyze(&requires.ast, &requires.settings, Some(&with_section.document));
    assert!(!result.errors.iter().any(|e| e.error_id == "SEM0002"), "{:?}", result.errors);
    assert!(result.section_results.contains_key("SECURITY"));

    // Neither has a section the tree can see: the error is reported.
    let (result, _) = analyze(&requires.ast, &requires.settings, None);
    assert!(result.errors.iter().any(|e| e.error_id == "SEM0002"));
    assert!(!result.is_success);

    // The document has no section: the error is reported even though the tree has one.
    let with_tree_section = parse(source("sec_dlm_requires_security"), ErrorHandlingStrategy::Continue).expect("parses");
    assert!(with_tree_section.ast.security.is_some());
    let (result, _) = analyze(&with_tree_section.ast, &with_tree_section.settings, Some(&without_section.document));
    assert!(result.errors.iter().any(|e| e.error_id == "SEM0002"), "{:?}", result.errors);
    assert!(!result.section_results.contains_key("SECURITY"));
}
