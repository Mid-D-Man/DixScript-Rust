//! KeywordDefinitions - Context-aware keyword management for DixScript

use std::sync::OnceLock;
use std::collections::HashSet as StdHashSet;

/// Keywords - Manages DixScript keywords with context awareness
pub struct Keywords;

// Static keyword sets (initialized once) - using std::collections::HashSet
static TRULY_RESERVED: OnceLock<StdHashSet<String>> = OnceLock::new();
static DATA_TYPE_KEYWORDS: OnceLock<StdHashSet<String>> = OnceLock::new();
static CONFIG_SECTION_KEYWORDS: OnceLock<StdHashSet<String>> = OnceLock::new();
static SECURITY_SECTION_KEYWORDS: OnceLock<StdHashSet<String>> = OnceLock::new();
static DLM_KEYWORDS: OnceLock<StdHashSet<String>> = OnceLock::new();
static CONFIG_VALUE_KEYWORDS: OnceLock<StdHashSet<String>> = OnceLock::new();
static CONTEXTUAL_IDENTIFIERS: OnceLock<StdHashSet<String>> = OnceLock::new();

impl Keywords {
    // ========== Truly Reserved Keywords ==========

    /// Returns truly reserved keywords (never allowed as identifiers)
    pub fn TrulyReservedKeywords() -> &'static StdHashSet<String> {
        TRULY_RESERVED.get_or_init(|| {
            let mut set = StdHashSet::new();
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

            set
        })
    }

    // ========== Data Type Keywords ==========

    /// Returns data type keywords (only special in type annotations)
    pub fn DataTypeKeywords() -> &'static StdHashSet<String> {
        DATA_TYPE_KEYWORDS.get_or_init(|| {
            let mut set = StdHashSet::new();
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
            set
        })
    }

    // ========== Section-Specific Keywords ==========

    /// Returns CONFIG section keywords
    pub fn ConfigSectionKeywords() -> &'static StdHashSet<String> {
        CONFIG_SECTION_KEYWORDS.get_or_init(|| {
            let mut set = StdHashSet::new();
            set.insert("version".to_string());
            set.insert("encoding".to_string());
            set.insert("author".to_string());
            set.insert("created".to_string());
            set.insert("features".to_string());
            set.insert("debug_mode".to_string());
            set.insert("error_handling".to_string());
            set.insert("compatibility_mode".to_string());
            set
        })
    }

    /// Returns SECURITY section keywords
    pub fn SecuritySectionKeywords() -> &'static StdHashSet<String> {
        SECURITY_SECTION_KEYWORDS.get_or_init(|| {
            let mut set = StdHashSet::new();
            set.insert("encryption".to_string());
            set.insert("validation".to_string());
            set.insert("keystore".to_string());
            set.insert("override".to_string());
            set.insert("metadata".to_string());
            set
        })
    }

    /// Returns DLM keywords
    pub fn DLMKeywords() -> &'static StdHashSet<String> {
        DLM_KEYWORDS.get_or_init(|| {
            let mut set = StdHashSet::new();
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
        })
    }

    /// Returns CONFIG value keywords
    pub fn ConfigValueKeywords() -> &'static StdHashSet<String> {
        CONFIG_VALUE_KEYWORDS.get_or_init(|| {
            let mut set = StdHashSet::new();
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
        })
    }

    /// Returns contextual identifiers (special in certain contexts)
    pub fn ContextualIdentifiers() -> &'static StdHashSet<String> {
        CONTEXTUAL_IDENTIFIERS.get_or_init(|| {
            let mut set = StdHashSet::new();
            set.insert("config".to_string());
            set.insert("Dix".to_string());
            set
        })
    }

    // ========== Context-Aware Checks ==========

    /// Checks if a word is reserved in a given context
    pub fn IsReservedInContext(word: &str, context: &str) -> bool {
        // Truly reserved keywords are ALWAYS reserved
        if Self::TrulyReservedKeywords().contains(word) {
            return true;
        }

        // Data type keywords are NOT reserved in QUICKFUNCS/DATA contexts
        if Self::DataTypeKeywords().contains(word) {
            let context_upper = context.to_uppercase();
            if context_upper == "QUICKFUNCS" || context_upper == "DATA" {
                return false; // NOT reserved - can be identifier
            }
            return false; // For now, allow everywhere except where truly reserved
        }

        // Section-specific keywords are only reserved in their own section
        let context_upper = context.to_uppercase();
        match context_upper.as_str() {
            "CONFIG" => {
                Self::ConfigSectionKeywords().contains(word)
                    || Self::ConfigValueKeywords().contains(word)
            }
            "SECURITY" => Self::SecuritySectionKeywords().contains(word),
            "DLM" => Self::DLMKeywords().contains(word),
            "QUICKFUNCS" => false, // Section keywords allowed here
            "DATA" => false,       // Section keywords allowed here
            _ => false,
        }
    }

    /// Checks if word can be used as an identifier in given context
    pub fn CanBeIdentifierInContext(word: &str, context: &str) -> bool {
        !Self::IsReservedInContext(word, context)
    }

    /// Checks if word is a contextual identifier
    pub fn IsContextualIdentifier(word: &str) -> bool {
        Self::ContextualIdentifiers().contains(word)
    }

    /// Checks if word is a data type keyword
    pub fn IsDataTypeKeyword(word: &str) -> bool {
        Self::DataTypeKeywords().contains(word)
    }

    /// Checks if word is any kind of keyword
    pub fn IsKeyword(word: &str) -> bool {
        Self::TrulyReservedKeywords().contains(word)
            || Self::DataTypeKeywords().contains(word)
            || Self::ConfigSectionKeywords().contains(word)
            || Self::SecuritySectionKeywords().contains(word)
            || Self::DLMKeywords().contains(word)
            || Self::ConfigValueKeywords().contains(word)
    }

    /// Gets the category of a keyword (for error messages)
    pub fn GetKeywordCategory(word: &str) -> String {
        if Self::TrulyReservedKeywords().contains(word) {
            "Reserved Keyword".to_string()
        } else if Self::DataTypeKeywords().contains(word) {
            "Data Type Keyword (can be identifier)".to_string()
        } else if Self::ConfigSectionKeywords().contains(word) {
            "Config Keyword".to_string()
        } else if Self::SecuritySectionKeywords().contains(word) {
            "Security Keyword".to_string()
        } else if Self::DLMKeywords().contains(word) {
            "DLM Keyword".to_string()
        } else if Self::ConfigValueKeywords().contains(word) {
            "Config Value Keyword".to_string()
        } else if Self::ContextualIdentifiers().contains(word) {
            "Contextual Identifier".to_string()
        } else {
            "Unknown".to_string()
        }
    }

    /// Checks if keyword is truly reserved (always)
    pub fn IsTrulyReservedKeyword(word: &str) -> bool {
        Self::TrulyReservedKeywords().contains(word)
    }

    /// Gets helpful error message for keyword usage
    pub fn GetKeywordUsageError(word: &str, context: &str) -> String {
        if Self::TrulyReservedKeywords().contains(word) {
            format!(
                "'{}' is a reserved keyword and cannot be used as an identifier",
                word
            )
        } else if Self::DataTypeKeywords().contains(word) {
            format!(
                "'{}' is a data type keyword but can be used as an identifier in {}",
                word, context
            )
        } else if Self::ConfigSectionKeywords().contains(word) && context == "CONFIG" {
            format!(
                "'{}' is a CONFIG section keyword and cannot be used here",
                word
            )
        } else if Self::SecuritySectionKeywords().contains(word)
            && context == "SECURITY"
        {
            format!(
                "'{}' is a SECURITY section keyword and cannot be used here",
                word
            )
        } else if Self::DLMKeywords().contains(word) && context == "DLM" {
            format!("'{}' is a DLM keyword and cannot be used here", word)
        } else {
            format!("'{}' can be used as an identifier in {} section", word, context)
        }
    }

    /// Checks if word is a section keyword (e.g., @CONFIG, @DATA)
    pub fn IsSectionKeyword(word: &str) -> bool {
        matches!(
            word.to_uppercase().as_str(),
            "@CONFIG" | "@DLM" | "@ENUMS" | "@QUICKFUNCS" | "@DATA" | "@SECURITY"
        )
    }

    /// Gets list of valid section keywords
    pub fn GetValidSectionKeywords() -> Vec<String> {
        vec![
            "@CONFIG".to_string(),
            "@DLM".to_string(),
            "@ENUMS".to_string(),
            "@QUICKFUNCS".to_string(),
            "@DATA".to_string(),
            "@SECURITY".to_string(),
        ]
    }

    /// Checks if word is a control flow keyword
    pub fn IsControlFlowKeyword(word: &str) -> bool {
        matches!(
            word,
            "if" | "elif" | "else" | "chk" | "miss" | "then" | "return" | "log"
        )
    }
}