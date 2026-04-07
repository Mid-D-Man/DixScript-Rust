

// Context-aware keyword management for DixScript v1.0.0.
//
// ## Why PHF instead of LazyLock<HashSet<String>>
//
// The previous implementation paid three runtime costs on every cold path:
//   1. A `OnceLock`/`LazyLock` check on the first call to each category.
//   2. Heap allocation of a `HashSet<String>` per category.
//   3. Heap allocation of every keyword `String` inside each set.
//
// `phf::Map<&'static str, ()>` eliminates all three:
//   - The map lives entirely in the binary's read-only data segment.
//   - Zero runtime initialisation — no lock, no `Vec`, no `String`.
//   - Lookups are O(1) via compile-time-generated perfect hashing.
//
// ## Lookup pattern
//
// PHF maps are case-sensitive.  All keys are stored lowercase.
// Callers pass arbitrary-case identifiers; methods lowercase once before
// the lookup (`word.to_lowercase()` allocates one `String`).  This is
// acceptable because keyword checks are never in the lexer hot path —
// they run in semantic analysis and error reporting.
//
// ## Sharing with the lexer's PHF
//
// The lexer has its own `phf::Map<&'static str, fn() -> TokenType>` for
// tokenisation.  These two maps cannot be merged: different value types,
// different purpose.  Both use PHF; neither depends on the other.

use phf::{phf_map, Map};

// =============================================================================
// Module-level static PHF maps
// =============================================================================

/// Control-flow, logical, literal, scope, declaration, and import keywords
/// that are NEVER valid identifiers in any context.
static TRULY_RESERVED: Map<&'static str, ()> = phf_map! {
    // Control flow
    "if"         => (),
    "elif"       => (),
    "else"       => (),
    "chk"        => (),
    "miss"       => (),
    "then"       => (),
    "return"     => (),
    // Logical operators (word form)
    "and"        => (),
    "or"         => (),
    "not"        => (),
    // Literals
    "true"       => (),
    "false"      => (),
    "null"       => (),
    // Scope
    "global"     => (),
    // Variable declaration
    "const"      => (),
    "let"        => (),
    "mut"        => (),
    // Imports
    "from"       => (),
    "from_cloud" => (),
    "verify"     => (),
};

/// Data-type annotation keywords.  These appear inside `<…>` and as
/// function-parameter type annotations.  They *can* be property names in
/// `@DATA` but not variable/parameter names in `@QUICKFUNCS`.
static DATA_TYPE_KEYWORDS: Map<&'static str, ()> = phf_map! {
    "int"       => (),
    "float"     => (),
    "double"    => (),
    "string"    => (),
    "bool"      => (),
    "array"     => (),
    "tuple"     => (),
    "hex"       => (),
    "blob"      => (),
    "regex"     => (),
    "object"    => (),
    "timestamp" => (),
    "date"      => (),
    "enum"      => (),
    "any"       => (),
};

/// All language-level keywords (truly-reserved ∪ data-type).
/// Used by `is_keyword()` as a single O(1) lookup instead of two sequential
/// map checks.
static ALL_LANGUAGE_KEYWORDS: Map<&'static str, ()> = phf_map! {
    // --- truly reserved ---
    "if"         => (), "elif"       => (), "else"  => (),
    "chk"        => (), "miss"       => (), "then"  => (),
    "return"     => (), "and"        => (), "or"    => (),
    "not"        => (), "true"       => (), "false" => (),
    "null"       => (), "global"     => (), "const" => (),
    "let"        => (), "mut"        => (), "from"  => (),
    "from_cloud" => (), "verify"     => (),
    // --- data-type ---
    "int"       => (), "float"     => (), "double"    => (),
    "string"    => (), "bool"      => (), "array"     => (),
    "tuple"     => (), "hex"       => (), "blob"      => (),
    "regex"     => (), "object"    => (), "timestamp" => (),
    "date"      => (), "enum"      => (), "any"       => (),
};

/// Keys recognised inside `@CONFIG( … )`.
static CONFIG_SECTION_KEYWORDS: Map<&'static str, ()> = phf_map! {
    "version"            => (),
    "encoding"           => (),
    "author"             => (),
    "created"            => (),
    "features"           => (),
    "debug_mode"         => (),
    "error_handling"     => (),
    "compatibility_mode" => (),
};

/// Block keys recognised inside `@SECURITY( … )`.
static SECURITY_SECTION_KEYWORDS: Map<&'static str, ()> = phf_map! {
    "encryption" => (),
    "validation" => (),
    "keystore"   => (),
    "override"   => (),
    "metadata"   => (),
};

/// Module identifiers recognised inside `@DLM( … )`.
static DLM_KEYWORDS: Map<&'static str, ()> = phf_map! {
    // Module types
    "dcompressor" => (),
    "dauditor"    => (),
    "dencryptor"  => (),
    // DCompressor subtypes
    "gzip"        => (),
    "bzip2"       => (),
    "lzma"        => (),
    // DAuditor subtypes
    "diy"         => (),
    "enhanced"    => (),
    // DEncryptor subtypes
    "xor"         => (),
    "aes128"      => (),
    "aes256"      => (),
    "chacha20"    => (),
};

/// Value-side keywords for `@CONFIG` entries (not keys, values).
static CONFIG_VALUE_KEYWORDS: Map<&'static str, ()> = phf_map! {
    // Error handling strategies
    "halt"        => (),
    "continue"    => (),
    "recover"     => (),
    // Compatibility modes
    "strict"      => (),
    "best_effort" => (),
    "permissive"  => (),
    // Debug modes
    "off"         => (),
    "regular"     => (),
    "verbose"     => (),
    // Feature values
    "basic"       => (),
    "advanced"    => (),
    "quickfuncs"  => (),
    "enums"       => (),
    "dlm"         => (),
    "data"        => (),
};

/// Identifiers with special meaning in specific expression contexts
/// (not reserved, but treated specially by the parser).
static CONTEXTUAL_IDENTIFIERS: Map<&'static str, ()> = phf_map! {
    "config" => (),
    "dix"    => (),
};

/// Combined map covering every keyword from every category.
/// Used by `is_keyword()` so it never needs to check multiple maps.
static ALL_KEYWORDS_COMBINED: Map<&'static str, ()> = phf_map! {
    // --- truly reserved ---
    "if" => (), "elif" => (), "else" => (), "chk" => (), "miss" => (),
    "then" => (), "return" => (), "and" => (), "or" => (), "not" => (),
    "true" => (), "false" => (), "null" => (), "global" => (),
    "const" => (), "let" => (), "mut" => (), "from" => (),
    "from_cloud" => (), "verify" => (),
    // --- data type ---
    "int" => (), "float" => (), "double" => (), "string" => (), "bool" => (),
    "array" => (), "tuple" => (), "hex" => (), "blob" => (), "regex" => (),
    "object" => (), "timestamp" => (), "date" => (), "enum" => (), "any" => (),
    // --- config keys ---
    "version" => (), "encoding" => (), "author" => (), "created" => (),
    "features" => (), "debug_mode" => (), "error_handling" => (),
    "compatibility_mode" => (),
    // --- config values ---
    "halt" => (), "continue" => (), "recover" => (), "strict" => (),
    "best_effort" => (), "permissive" => (), "off" => (), "regular" => (),
    "verbose" => (), "basic" => (), "advanced" => (), "quickfuncs" => (),
    "enums" => (), "dlm" => (), "data" => (),
    // --- security ---
    "encryption" => (), "validation" => (), "keystore" => (),
    "override" => (), "metadata" => (),
    // --- dlm (lowercased) ---
    "dcompressor" => (), "dauditor" => (), "dencryptor" => (),
    "gzip" => (), "bzip2" => (), "lzma" => (), "diy" => (), "enhanced" => (),
    "xor" => (), "aes128" => (), "aes256" => (), "chacha20" => (),
};

// =============================================================================
// Helper — case-insensitive PHF lookup
// =============================================================================

/// Look up `word` (any case) in a `phf::Map<&'static str, ()>` whose
/// keys are all lowercase.  Allocates one `String` for the lowercase
/// conversion, then does an O(1) PHF lookup.
///
/// The `Borrow<str>` bound on `Map<&'static str, ()>` means we can pass
/// `&str` directly to `contains_key` — the lifetime difference is erased
/// by the Borrow impl.
#[inline]
fn map_contains_ci(map: &Map<&'static str, ()>, word: &str) -> bool {
    let lower = word.to_lowercase();
    map.contains_key(&*lower)
}

// =============================================================================
// Public API — Keywords struct
// =============================================================================

/// Context-aware keyword management for DixScript v1.0.0.
pub struct Keywords;

impl Keywords {
    // ------------------------------------------------------------------
    // Map accessors — return the static PHF maps directly.
    // Callers that previously iterated with .iter().any(…) should now
    // use .contains_key(…) for O(1) lookup, or use the boolean helpers
    // below which already do that internally.
    // ------------------------------------------------------------------

    /// Keywords that are never valid identifiers anywhere.
    #[inline]
    pub fn truly_reserved_keywords() -> &'static Map<&'static str, ()> {
        &TRULY_RESERVED
    }

    /// Keywords that are only special in type annotations.
    #[inline]
    pub fn data_type_keywords() -> &'static Map<&'static str, ()> {
        &DATA_TYPE_KEYWORDS
    }

    /// All language keywords (truly-reserved ∪ data-type).
    #[inline]
    pub fn language_keywords() -> &'static Map<&'static str, ()> {
        &ALL_LANGUAGE_KEYWORDS
    }

    /// Keys valid inside `@CONFIG(…)`.
    #[inline]
    pub fn config_section_keywords() -> &'static Map<&'static str, ()> {
        &CONFIG_SECTION_KEYWORDS
    }

    /// Block keys valid inside `@SECURITY(…)`.
    #[inline]
    pub fn security_section_keywords() -> &'static Map<&'static str, ()> {
        &SECURITY_SECTION_KEYWORDS
    }

    /// Module identifiers valid inside `@DLM(…)`.
    #[inline]
    pub fn dlm_keywords() -> &'static Map<&'static str, ()> {
        &DLM_KEYWORDS
    }

    /// Value-side keywords for `@CONFIG` entries.
    #[inline]
    pub fn config_value_keywords() -> &'static Map<&'static str, ()> {
        &CONFIG_VALUE_KEYWORDS
    }

    /// Identifiers with special expression-context meaning.
    #[inline]
    pub fn contextual_identifiers() -> &'static Map<&'static str, ()> {
        &CONTEXTUAL_IDENTIFIERS
    }

    // ------------------------------------------------------------------
    // Boolean helpers — these are the recommended call sites.
    // All comparisons are case-insensitive.
    // ------------------------------------------------------------------

    /// `true` if `word` is a data-type annotation keyword
    /// ("int", "float", "string", …).
    #[inline]
    pub fn is_data_type_keyword(word: &str) -> bool {
        map_contains_ci(&DATA_TYPE_KEYWORDS, word)
    }

    /// `true` if `word` is any keyword in any category.
    /// Single O(1) PHF lookup into the combined map.
    #[inline]
    pub fn is_keyword(word: &str) -> bool {
        map_contains_ci(&ALL_KEYWORDS_COMBINED, word)
    }

    /// `true` if `word` is a contextual identifier (`config`, `Dix`).
    #[inline]
    pub fn is_contextual_identifier(word: &str) -> bool {
        map_contains_ci(&CONTEXTUAL_IDENTIFIERS, word)
    }

    /// `true` if `word` is a control-flow keyword
    /// ("if", "elif", "else", "chk", "miss", "then", "return", "log").
    pub fn is_control_flow_keyword(word: &str) -> bool {
        matches!(
            word.to_lowercase().as_str(),
            "if" | "elif" | "else" | "chk" | "miss" | "then" | "return" | "log"
        )
    }

    /// `true` if `word` is a top-level section keyword (`@CONFIG`, `@DATA`, …).
    pub fn is_section_keyword(word: &str) -> bool {
        matches!(
            word.to_uppercase().as_str(),
            "@CONFIG" | "@DLM" | "@ENUMS" | "@IMPORTS"
            | "@QUICKFUNCS" | "@DATA" | "@SECURITY"
        )
    }

    /// Returns all valid section keyword strings.
    pub fn get_valid_section_keywords() -> &'static [&'static str] {
        &[
            "@CONFIG", "@IMPORTS", "@DLM", "@ENUMS",
            "@QUICKFUNCS", "@DATA", "@SECURITY",
        ]
    }

    // ------------------------------------------------------------------
    // Context-aware reservation check
    // ------------------------------------------------------------------

    /// `true` if `word` must not be used as an identifier in `context`.
    ///
    /// `context` is the section name: `"QUICKFUNCS"`, `"DATA"`,
    /// `"CONFIG"`, `"SECURITY"`, `"DLM"`, etc.
    pub fn is_reserved_in_context(word: &str, context: &str) -> bool {
        let context_upper = context.to_uppercase();

        // Truly-reserved keywords block everywhere.
        if map_contains_ci(&TRULY_RESERVED, word) {
            return true;
        }

        // Data-type keywords are reserved as variable/parameter names in
        // QUICKFUNCS, but allowed as property names in DATA.
        if map_contains_ci(&DATA_TYPE_KEYWORDS, word) {
            return context_upper == "QUICKFUNCS";
        }

        // Section-specific keyword sets.
        match context_upper.as_str() {
            "CONFIG" => {
                map_contains_ci(&CONFIG_SECTION_KEYWORDS, word)
                    || map_contains_ci(&CONFIG_VALUE_KEYWORDS, word)
            }
            "SECURITY" => map_contains_ci(&SECURITY_SECTION_KEYWORDS, word),
            "DLM"      => map_contains_ci(&DLM_KEYWORDS, word),
            _          => false,
        }
    }

    /// Inverse of `is_reserved_in_context`.
    #[inline]
    pub fn can_be_identifier_in_context(word: &str, context: &str) -> bool {
        !Keywords::is_reserved_in_context(word, context)
    }

    // ------------------------------------------------------------------
    // Diagnostics
    // ------------------------------------------------------------------

    /// Category label for `word`, used in error messages.
    pub fn get_keyword_category(word: &str) -> &'static str {
        if map_contains_ci(&TRULY_RESERVED, word)           { return "Reserved Keyword"; }
        if map_contains_ci(&DATA_TYPE_KEYWORDS, word)       { return "Data Type Keyword (can be identifier)"; }
        if map_contains_ci(&CONFIG_SECTION_KEYWORDS, word)  { return "Config Keyword"; }
        if map_contains_ci(&SECURITY_SECTION_KEYWORDS, word){ return "Security Keyword"; }
        if map_contains_ci(&DLM_KEYWORDS, word)             { return "DLM Keyword"; }
        if map_contains_ci(&CONFIG_VALUE_KEYWORDS, word)    { return "Config Value Keyword"; }
        if map_contains_ci(&CONTEXTUAL_IDENTIFIERS, word)   { return "Contextual Identifier"; }
        "Unknown"
    }

    /// Human-readable error message explaining why `word` cannot be used
    /// as an identifier in `context`.
    pub fn get_keyword_usage_error(word: &str, context: &str) -> String {
        if map_contains_ci(&TRULY_RESERVED, word) {
            return format!(
                "'{}' is a reserved keyword and cannot be used as an identifier",
                word
            );
        }

        if map_contains_ci(&DATA_TYPE_KEYWORDS, word) {
            if context == "QUICKFUNCS" {
                let capitalized = {
                    let mut c = word.chars();
                    match c.next() {
                        None    => String::new(),
                        Some(f) => f.to_uppercase().to_string() + c.as_str(),
                    }
                };
                return format!(
                    "'{}' is a data type keyword and cannot be used as a variable or \
                     parameter name. Consider: 'my{}' or '{}Value'",
                    word, capitalized, word
                );
            }
            return format!(
                "'{}' is a data type keyword but can be used as a property name in {}",
                word, context
            );
        }

        let context_upper = context.to_uppercase();

        if map_contains_ci(&CONFIG_SECTION_KEYWORDS, word) && context_upper == "CONFIG" {
            return format!("'{}' is a CONFIG section keyword and cannot be used here", word);
        }
        if map_contains_ci(&SECURITY_SECTION_KEYWORDS, word) && context_upper == "SECURITY" {
            return format!("'{}' is a SECURITY section keyword and cannot be used here", word);
        }
        if map_contains_ci(&DLM_KEYWORDS, word) && context_upper == "DLM" {
            return format!("'{}' is a DLM keyword and cannot be used here", word);
        }

        format!("'{}' can be used as an identifier in the {} section", word, context)
    }
        }
