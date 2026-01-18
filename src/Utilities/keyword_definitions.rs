use crate::DixCore::HashSet;
use std::collections::HashMap;
use std::sync::LazyLock;

/// Context-aware keyword management for DixScript v1.0.0
/// Maintains C# naming convention (PascalCase) for compatibility
pub struct Keywords;

impl Keywords {
    /// TRULY RESERVED KEYWORDS - Never allowed as identifiers anywhere
    pub fn truly_reserved_keywords() -> &'static HashSet<String> {
        static KEYWORDS: LazyLock<HashSet<String>> = LazyLock::new(|| {
            let mut set = HashSet::New();
            // Control flow
            set.Add("if".to_string());
            set.Add("elif".to_string());
            set.Add("else".to_string());
            set.Add("chk".to_string());
            set.Add("miss".to_string());
            set.Add("then".to_string());
            set.Add("return".to_string());

            // Logical operators
            set.Add("and".to_string());
            set.Add("or".to_string());
            set.Add("not".to_string());

            // Literals
            set.Add("true".to_string());
            set.Add("false".to_string());
            set.Add("null".to_string());

            // Scope keywords
            set.Add("global".to_string());

            // Variable declaration keywords
            set.Add("const".to_string());
            set.Add("let".to_string());
            set.Add("mut".to_string());

            // Imports keywords
            set.Add("from".to_string());
            set.Add("from_cloud".to_string());
            set.Add("verify".to_string());

            set
        });
        &KEYWORDS
    }

    /// DATA TYPE KEYWORDS - Only special in type annotations
    pub fn data_type_keywords() -> &'static HashSet<String> {
        static KEYWORDS: LazyLock<HashSet<String>> = LazyLock::new(|| {
            let mut set = HashSet::New();
            set.Add("int".to_string());
            set.Add("float".to_string());
            set.Add("double".to_string());
            set.Add("string".to_string());
            set.Add("bool".to_string());
            set.Add("array".to_string());
            set.Add("tuple".to_string());
            set.Add("hex".to_string());
            set.Add("blob".to_string());
            set.Add("regex".to_string());
            set.Add("object".to_string());
            set.Add("timestamp".to_string());
            set.Add("date".to_string());
            set.Add("enum".to_string());
            set.Add("any".to_string());
            set
        });
        &KEYWORDS
    }

    /// ALL LANGUAGE KEYWORDS - Combined set
    pub fn language_keywords() -> &'static HashSet<String> {
        static KEYWORDS: LazyLock<HashSet<String>> = LazyLock::new(|| {
            let mut set = HashSet::New();
            for kw in Self::truly_reserved_keywords().Iter() {
                set.Add(kw.clone());
            }
            for kw in Self::data_type_keywords().Iter() {
                set.Add(kw.clone());
            }
            set
        });
        &KEYWORDS
    }

    /// CONFIG SECTION KEYWORDS
    pub fn config_section_keywords() -> &'static HashSet<String> {
        static KEYWORDS: LazyLock<HashSet<String>> = LazyLock::new(|| {
            let mut set = HashSet::New();
            set.Add("version".to_string());
            set.Add("encoding".to_string());
            set.Add("author".to_string());
            set.Add("created".to_string());
            set.Add("features".to_string());
            set.Add("debug_mode".to_string());
            set.Add("error_handling".to_string());
            set.Add("compatibility_mode".to_string());
            set
        });
        &KEYWORDS
    }

    /// SECURITY SECTION KEYWORDS
    pub fn security_section_keywords() -> &'static HashSet<String> {
        static KEYWORDS: LazyLock<HashSet<String>> = LazyLock::new(|| {
            let mut set = HashSet::New();
            set.Add("encryption".to_string());
            set.Add("validation".to_string());
            set.Add("keystore".to_string());
            set.Add("override".to_string());
            set.Add("metadata".to_string());
            set
        });
        &KEYWORDS
    }

    /// DLM MODULE KEYWORDS
    pub fn dlm_keywords() -> &'static HashSet<String> {
        static KEYWORDS: LazyLock<HashSet<String>> = LazyLock::new(|| {
            let mut set = HashSet::New();
            // Module types
            set.Add("DCompressor".to_string());
            set.Add("DAuditor".to_string());
            set.Add("DEncryptor".to_string());

            // DCompressor subtypes
            set.Add("gzip".to_string());
            set.Add("bzip2".to_string());
            set.Add("lzma".to_string());

            // DAuditor subtypes
            set.Add("diy".to_string());
            set.Add("enhanced".to_string());

            // DEncryptor subtypes
            set.Add("xor".to_string());
            set.Add("aes128".to_string());
            set.Add("aes256".to_string());
            set.Add("chacha20".to_string());
            set
        });
        &KEYWORDS
    }

    /// CONFIG VALUE KEYWORDS
    pub fn config_value_keywords() -> &'static HashSet<String> {
        static KEYWORDS: LazyLock<HashSet<String>> = LazyLock::new(|| {
            let mut set = HashSet::New();
            // Error handling strategies
            set.Add("halt".to_string());
            set.Add("continue".to_string());
            set.Add("recover".to_string());

            // Compatibility modes
            set.Add("strict".to_string());
            set.Add("best_effort".to_string());
            set.Add("permissive".to_string());

            // Debug modes
            set.Add("off".to_string());
            set.Add("regular".to_string());
            set.Add("verbose".to_string());

            // Feature values
            set.Add("basic".to_string());
            set.Add("advanced".to_string());
            set.Add("quickfuncs".to_string());
            set.Add("enums".to_string());
            set.Add("dlm".to_string());
            set.Add("data".to_string());
            set
        });
        &KEYWORDS
    }

    /// CONTEXTUAL IDENTIFIERS
    pub fn contextual_identifiers() -> &'static HashSet<String> {
        static KEYWORDS: LazyLock<HashSet<String>> = LazyLock::new(|| {
            let mut set = HashSet::New();
            set.Add("config".to_string());
            set.Add("Dix".to_string());
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
        if Self::truly_reserved_keywords()
            .Iter()
            .any(|k| k.to_lowercase() == word_lower)
        {
            return true;
        }

        // Data type keywords CANNOT be used as variable/parameter names in QUICKFUNCS
        if Self::data_type_keywords()
            .Iter()
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
                Self::config_section_keywords()
                    .Iter()
                    .any(|k| k.to_lowercase() == word_lower)
                    || Self::config_value_keywords()
                    .Iter()
                    .any(|k| k.to_lowercase() == word_lower)
            }
            "SECURITY" => Self::security_section_keywords()
                .Iter()
                .any(|k| k.to_lowercase() == word_lower),
            "DLM" => Self::dlm_keywords()
                .Iter()
                .any(|k| k.to_lowercase() == word_lower),
            "QUICKFUNCS" | "DATA" => false,
            _ => false,
        }
    }

    /// Check if word can be used as identifier in context
    pub fn can_be_identifier_in_context(word: &str, context: &str) -> bool {
        !Self::is_reserved_in_context(word, context)
    }

    /// Check if word is a contextual identifier
    pub fn is_contextual_identifier(word: &str) -> bool {
        Self::contextual_identifiers()
            .Iter()
            .any(|k| k.to_lowercase() == word.to_lowercase())
    }

    /// Check if word is a data type keyword
    pub fn is_data_type_keyword(word: &str) -> bool {
        Self::data_type_keywords()
            .Iter()
            .any(|k| k.to_lowercase() == word.to_lowercase())
    }

    /// Check if word is any kind of keyword
    pub fn is_keyword(word: &str) -> bool {
        let word_lower = word.to_lowercase();
        Self::language_keywords()
            .Iter()
            .any(|k| k.to_lowercase() == word_lower)
            || Self::config_section_keywords()
            .Iter()
            .any(|k| k.to_lowercase() == word_lower)
            || Self::security_section_keywords()
            .Iter()
            .any(|k| k.to_lowercase() == word_lower)
            || Self::dlm_keywords()
            .Iter()
            .any(|k| k.to_lowercase() == word_lower)
            || Self::config_value_keywords()
            .Iter()
            .any(|k| k.to_lowercase() == word_lower)
    }

    /// Get keyword category
    pub fn get_keyword_category(word: &str) -> &'static str {
        let word_lower = word.to_lowercase();

        if Self::truly_reserved_keywords()
            .Iter()
            .any(|k| k.to_lowercase() == word_lower)
        {
            return "Reserved Keyword";
        }
        if Self::data_type_keywords()
            .Iter()
            .any(|k| k.to_lowercase() == word_lower)
        {
            return "Data Type Keyword (can be identifier)";
        }
        if Self::config_section_keywords()
            .Iter()
            .any(|k| k.to_lowercase() == word_lower)
        {
            return "Config Keyword";
        }
        if Self::security_section_keywords()
            .Iter()
            .any(|k| k.to_lowercase() == word_lower)
        {
            return "Security Keyword";
        }
        if Self::dlm_keywords()
            .Iter()
            .any(|k| k.to_lowercase() == word_lower)
        {
            return "DLM Keyword";
        }
        if Self::config_value_keywords()
            .Iter()
            .any(|k| k.to_lowercase() == word_lower)
        {
            return "Config Value Keyword";
        }
        if Self::contextual_identifiers()
            .Iter()
            .any(|k| k.to_lowercase() == word_lower)
        {
            return "Contextual Identifier";
        }

        "Unknown"
    }

    /// Get helpful error message
    pub fn get_keyword_usage_error(word: &str, context: &str) -> String {
        let word_lower = word.to_lowercase();

        if Self::truly_reserved_keywords()
            .Iter()
            .any(|k| k.to_lowercase() == word_lower)
        {
            return format!(
                "'{}' is a reserved keyword and cannot be used as an identifier",
                word
            );
        }

        if Self::data_type_keywords()
            .Iter()
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

        if Self::config_section_keywords()
            .Iter()
            .any(|k| k.to_lowercase() == word_lower)
            && context == "CONFIG"
        {
            return format!(
                "'{}' is a CONFIG section keyword and cannot be used here",
                word
            );
        }

        if Self::security_section_keywords()
            .Iter()
            .any(|k| k.to_lowercase() == word_lower)
            && context == "SECURITY"
        {
            return format!(
                "'{}' is a SECURITY section keyword and cannot be used here",
                word
            );
        }

        if Self::dlm_keywords()
            .Iter()
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