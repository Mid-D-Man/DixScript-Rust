use std::collections::HashSet;
use std::sync::LazyLock;

/// Context-aware keyword management for DixScript v1.0.0
/// Uses Rust naming conventions (snake_case methods)
pub struct Keywords;

impl Keywords {
    /// TRULY RESERVED KEYWORDS - Never allowed as identifiers anywhere
    pub fn truly_reserved_keywords() -> &'static HashSet<String> {
        static KEYWORDS: LazyLock<HashSet<String>> = LazyLock::new(|| {
            let mut set = HashSet::new();
            // Control flow
            set.insert("if".to_string());
            set.insert("elif".to_string());
            set.insert("else".to_string());
            set.insert("chk".to_string());
            set.insert("miss".to_string());
            set.insert("then".to_string());
            set.insert("return".to_string());

            // Logical operators
            set.insert("and".to_string());
            set.insert("or".to_string());
            set.insert("not".to_string());

            // Literals
            set.insert("true".to_string());
            set.insert("false".to_string());
            set.insert("null".to_string());

            // Scope keywords
            set.insert("global".to_string());

            // Variable declaration keywords
            set.insert("const".to_string());
            set.insert("let".to_string());
            set.insert("mut".to_string());

            // Imports keywords
            set.insert("from".to_string());
            set.insert("from_cloud".to_string());
            set.insert("verify".to_string());

            set
        });
        &KEYWORDS
    }

    /// DATA TYPE KEYWORDS - Only special in type annotations
    pub fn data_type_keywords() -> &'static HashSet<String> {
        static KEYWORDS: LazyLock<HashSet<String>> = LazyLock::new(|| {
            let mut set = HashSet::new();
            set.insert("int".to_string());
            set.insert("float".to_string());
            set.insert("double".to_string());
            set.insert("string".to_string());
            set.insert("bool".to_string());
            set.insert("array".to_string());
            set.insert("tuple".to_string());
            set.insert("hex".to_string());
            set.insert("blob".to_string());
            set.insert("regex".to_string());
            set.insert("object".to_string());
            set.insert("timestamp".to_string());
            set.insert("date".to_string());
            set.insert("enum".to_string());
            set.insert("any".to_string());
            set
        });
        &KEYWORDS
    }

    /// ALL LANGUAGE KEYWORDS - Combined set
    pub fn language_keywords() -> &'static HashSet<String> {
        static KEYWORDS: LazyLock<HashSet<String>> = LazyLock::new(|| {
            let mut set = HashSet::new();
            for kw in Keywords::truly_reserved_keywords().iter() {
                set.insert(kw.clone());
            }
            for kw in Keywords::data_type_keywords().iter() {
                set.insert(kw.clone());
            }
            set
        });
        &KEYWORDS
    }

    /// CONFIG SECTION KEYWORDS
    pub fn config_section_keywords() -> &'static HashSet<String> {
        static KEYWORDS: LazyLock<HashSet<String>> = LazyLock::new(|| {
            let mut set = HashSet::new();
            set.insert("version".to_string());
            set.insert("encoding".to_string());
            set.insert("author".to_string());
            set.insert("created".to_string());
            set.insert("features".to_string());
            set.insert("debug_mode".to_string());
            set.insert("error_handling".to_string());
            set.insert("compatibility_mode".to_string());
            set
        });
        &KEYWORDS
    }

    /// SECURITY SECTION KEYWORDS
    pub fn security_section_keywords() -> &'static HashSet<String> {
        static KEYWORDS: LazyLock<HashSet<String>> = LazyLock::new(|| {
            let mut set = HashSet::new();
            set.insert("encryption".to_string());
            set.insert("validation".to_string());
            set.insert("keystore".to_string());
            set.insert("override".to_string());
            set.insert("metadata".to_string());
            set
        });
        &KEYWORDS
    }

    /// DLM MODULE KEYWORDS
    pub fn dlm_keywords() -> &'static HashSet<String> {
        static KEYWORDS: LazyLock<HashSet<String>> = LazyLock::new(|| {
            let mut set = HashSet::new();
            // Module types
            set.insert("DCompressor".to_string());
            set.insert("DAuditor".to_string());
            set.insert("DEncryptor".to_string());

            // DCompressor subtypes
            set.insert("gzip".to_string());
            set.insert("bzip2".to_string());
            set.insert("lzma".to_string());

            // DAuditor subtypes
            set.insert("diy".to_string());
            set.insert("enhanced".to_string());

            // DEncryptor subtypes
            set.insert("xor".to_string());
            set.insert("aes128".to_string());
            set.insert("aes256".to_string());
            set.insert("chacha20".to_string());
            set
        });
        &KEYWORDS
    }

    /// CONFIG VALUE KEYWORDS
    pub fn config_value_keywords() -> &'static HashSet<String> {
        static KEYWORDS: LazyLock<HashSet<String>> = LazyLock::new(|| {
            let mut set = HashSet::new();
            // Error handling strategies
            set.insert("halt".to_string());
            set.insert("continue".to_string());
            set.insert("recover".to_string());

            // Compatibility modes
            set.insert("strict".to_string());
            set.insert("best_effort".to_string());
            set.insert("permissive".to_string());

            // Debug modes
            set.insert("off".to_string());
            set.insert("regular".to_string());
            set.insert("verbose".to_string());

            // Feature values
            set.insert("basic".to_string());
            set.insert("advanced".to_string());
            set.insert("quickfuncs".to_string());
            set.insert("enums".to_string());
            set.insert("dlm".to_string());
            set.insert("data".to_string());
            set
        });
        &KEYWORDS
    }

    /// CONTEXTUAL IDENTIFIERS
    pub fn contextual_identifiers() -> &'static HashSet<String> {
        static KEYWORDS: LazyLock<HashSet<String>> = LazyLock::new(|| {
            let mut set = HashSet::new();
            set.insert("config".to_string());
            set.insert("Dix".to_string());
            set
        });
        &KEYWORDS
    }

    /// Context-aware keyword check
    pub fn is_reserved_in_context(word: &str, context: &str) -> bool {
        // Case-insensitive comparison
        let word_lower = word.to_lowercase();
        let context_upper = context.to_uppercase();

        // Truly reserved keywords are ALWAYS reserved
        if Keywords::truly_reserved_keywords()
            .iter()
            .any(|k| k.to_lowercase() == word_lower)
        {
            return true;
        }

        // Data type keywords CANNOT be used as variable/parameter names in QUICKFUNCS
        if Keywords::data_type_keywords()
            .iter()
            .any(|k| k.to_lowercase() == word_lower)
        {
            if context_upper == "QUICKFUNCS" {
                return true; // RESERVED
            }
            // In DATA section, they can be property names
            if context_upper == "DATA" {
                return false;
            }
            return false;
        }

        // Section-specific keywords
        match context_upper.as_str() {
            "CONFIG" => {
                Keywords::config_section_keywords()
                    .iter()
                    .any(|k| k.to_lowercase() == word_lower)
                    || Keywords::config_value_keywords()
                    .iter()
                    .any(|k| k.to_lowercase() == word_lower)
            }
            "SECURITY" => Keywords::security_section_keywords()
                .iter()
                .any(|k| k.to_lowercase() == word_lower),
            "DLM" => Keywords::dlm_keywords()
                .iter()
                .any(|k| k.to_lowercase() == word_lower),
            "QUICKFUNCS" | "DATA" => false,
            _ => false,
        }
    }

    /// Check if word can be used as identifier in context
    pub fn can_be_identifier_in_context(word: &str, context: &str) -> bool {
        !Keywords::is_reserved_in_context(word, context)
    }

    /// Check if word is a contextual identifier
    pub fn is_contextual_identifier(word: &str) -> bool {
        Keywords::contextual_identifiers()
            .iter()
            .any(|k| k.to_lowercase() == word.to_lowercase())
    }

    /// Check if word is a data type keyword
    pub fn is_data_type_keyword(word: &str) -> bool {
        Keywords::data_type_keywords()
            .iter()
            .any(|k| k.to_lowercase() == word.to_lowercase())
    }

    /// Check if word is any kind of keyword
    pub fn is_keyword(word: &str) -> bool {
        let word_lower = word.to_lowercase();
        Keywords::language_keywords()
            .iter()
            .any(|k| k.to_lowercase() == word_lower)
            || Keywords::config_section_keywords()
            .iter()
            .any(|k| k.to_lowercase() == word_lower)
            || Keywords::security_section_keywords()
            .iter()
            .any(|k| k.to_lowercase() == word_lower)
            || Keywords::dlm_keywords()
            .iter()
            .any(|k| k.to_lowercase() == word_lower)
            || Keywords::config_value_keywords()
            .iter()
            .any(|k| k.to_lowercase() == word_lower)
    }

    /// Get keyword category
    pub fn get_keyword_category(word: &str) -> &'static str {
        let word_lower = word.to_lowercase();

        if Keywords::truly_reserved_keywords()
            .iter()
            .any(|k| k.to_lowercase() == word_lower)
        {
            return "Reserved Keyword";
        }
        if Keywords::data_type_keywords()
            .iter()
            .any(|k| k.to_lowercase() == word_lower)
        {
            return "Data Type Keyword (can be identifier)";
        }
        if Keywords::config_section_keywords()
            .iter()
            .any(|k| k.to_lowercase() == word_lower)
        {
            return "Config Keyword";
        }
        if Keywords::security_section_keywords()
            .iter()
            .any(|k| k.to_lowercase() == word_lower)
        {
            return "Security Keyword";
        }
        if Keywords::dlm_keywords()
            .iter()
            .any(|k| k.to_lowercase() == word_lower)
        {
            return "DLM Keyword";
        }
        if Keywords::config_value_keywords()
            .iter()
            .any(|k| k.to_lowercase() == word_lower)
        {
            return "Config Value Keyword";
        }
        if Keywords::contextual_identifiers()
            .iter()
            .any(|k| k.to_lowercase() == word_lower)
        {
            return "Contextual Identifier";
        }

        "Unknown"
    }

    /// Get helpful error message
    pub fn get_keyword_usage_error(word: &str, context: &str) -> String {
        let word_lower = word.to_lowercase();

        if Keywords::truly_reserved_keywords()
            .iter()
            .any(|k| k.to_lowercase() == word_lower)
        {
            return format!(
                "'{}' is a reserved keyword and cannot be used as an identifier",
                word
            );
        }

        if Keywords::data_type_keywords()
            .iter()
            .any(|k| k.to_lowercase() == word_lower)
        {
            if context == "QUICKFUNCS" {
                let capitalized = format!(
                    "{}{}",
                    word.chars().next().unwrap().to_uppercase(),
                    &word[1..]
                );
                return format!(
                    "'{}' is a data type keyword and cannot be used as a variable or parameter name. Use a different name like 'my{}' or '{}Value'",
                    word, capitalized, word
                );
            }
            return format!(
                "'{}' is a data type keyword but can be used as a property name in {}",
                word, context
            );
        }

        if Keywords::config_section_keywords()
            .iter()
            .any(|k| k.to_lowercase() == word_lower)
            && context == "CONFIG"
        {
            return format!(
                "'{}' is a CONFIG section keyword and cannot be used here",
                word
            );
        }

        if Keywords::security_section_keywords()
            .iter()
            .any(|k| k.to_lowercase() == word_lower)
            && context == "SECURITY"
        {
            return format!(
                "'{}' is a SECURITY section keyword and cannot be used here",
                word
            );
        }

        if Keywords::dlm_keywords()
            .iter()
            .any(|k| k.to_lowercase() == word_lower)
            && context == "DLM"
        {
            return format!("'{}' is a DLM keyword and cannot be used here", word);
        }

        format!("'{}' can be used as an identifier in {} section", word, context)
    }

    /// Check if word is a section keyword
    pub fn is_section_keyword(word: &str) -> bool {
        matches!(
            word.to_uppercase().as_str(),
            "@CONFIG" | "@DLM" | "@ENUMS" | "@IMPORTS" | "@QUICKFUNCS" | "@DATA" | "@SECURITY"
        )
    }

    /// Get valid section keywords
    pub fn get_valid_section_keywords() -> Vec<&'static str> {
        vec![
            "@CONFIG",
            "@IMPORTS",
            "@DLM",
            "@ENUMS",
            "@QUICKFUNCS",
            "@DATA",
            "@SECURITY",
        ]
    }

    /// Check if word is a control flow keyword
    pub fn is_control_flow_keyword(word: &str) -> bool {
        matches!(
            word.to_lowercase().as_str(),
            "if" | "elif" | "else" | "chk" | "miss" | "then" | "return" | "log"
        )
    }
}