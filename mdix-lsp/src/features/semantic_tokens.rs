// mdix-lsp/src/features/semantic_tokens.rs
//!
//! ## Token coloring scheme
//! - `TT_NAMESPACE`   — static object receivers (Math, DateTime, …) & import aliases
//! - `TT_FUNCTION`    — QuickFunc calls, static method calls (MOD_STATIC), table/group-array paths
//! - `TT_METHOD`      — instance method calls (.toUpper(), .push() etc.)
//! - `TT_PROPERTY`    — property access after dot (non-call), CONFIG keys, SECURITY keys
//! - `TT_MACRO`       — DLM module names (DCompressor, DEncryptor, DAuditor)
//! - `TT_DECORATOR`   — DLM subtype names (gzip, aes256, …)
//!
//! ## Table / group-array path coloring (symbol-table / AST driven)
//! Instead of token-level heuristics, we build a set of (line, col) positions
//! directly from `DataEntry::TableProperty` and `DataEntry::GroupArray` nodes in
//! the resolved AST.  Every Identifier at one of those positions — as well as
//! every subsequent token while `in_table_path_chain` is true — is coloured
//! TT_FUNCTION (yellow), matching the TablePath token color so the whole dotted
//! path appears uniformly yellow.
//!
//! ## Conservative dot coloring in @DATA
//! `after_instance_dot` is only set when `prev_token_has_known_type` is true,
//! i.e. the receiver immediately before the dot is a literal, a call result `()`,
//! an array result `[]`, or an identifier present in the semantic `type_index`.
//! Unknown identifiers (table-path first segments, enum names without symbol-table
//! entry, etc.) do NOT trigger spurious property/method coloring.

use std::collections::{HashMap, HashSet};
use std::panic;

use tower_lsp::lsp_types::{SemanticToken, SemanticTokens, SemanticTokensResult};
use dixscript::Compiler::AST::{DataEntry, DataType};
use dixscript::Compiler::Core::Tokenizer::{Token, TokenType};
use dixscript::Compiler::Core::Tokenizer::token::SectionId;
use crate::document::Document;

use crate::capabilities::{
    TT_KEYWORD, TT_STRING, TT_NUMBER, TT_OPERATOR, TT_VARIABLE,
    TT_FUNCTION, TT_TYPE, TT_ENUM_MEMBER, TT_COMMENT, TT_NAMESPACE,
    TT_PROPERTY, TT_PARAMETER, TT_MACRO, TT_DECORATOR,
    TT_REGEXP, TT_EVENT, TT_METHOD,
    MOD_DECLARATION, MOD_READONLY, MOD_STATIC,
};

// ── Known built-in static objects (colour receiver as TT_NAMESPACE) ───────────
const STATIC_OBJECT_NAMES: &[&str] = &[
    "Math", "DateTime", "Array", "Random", "Guid", "IpAddress", "Enum", "Dix",
];

// ── AST-driven table-path position extraction ─────────────────────────────────

/// Builds the set of `(1-based line, 1-based column)` positions that are the
/// **first identifier** of a `@DATA` table-property or group-array path.
///
/// Examples
///   `player.config: speed = 5`  →  position of `player`
///   `enemies:: item1, item2`    →  position of `enemies`
///
/// We use these positions to colour those identifiers TT_FUNCTION (yellow) and
/// to start `in_table_path_chain` so subsequent dotted segments are also yellow.
fn build_table_path_positions(doc: &Document) -> HashSet<(usize, usize)> {
    let mut positions = HashSet::new();
    let data = match doc.ast.as_ref().and_then(|a| a.data.as_ref()) {
        Some(d) => d,
        None    => return positions,
    };
    for entry in &data.entries {
        let pos = match entry {
            DataEntry::TableProperty { position, .. }
            | DataEntry::GroupArray  { position, .. } => *position,
            _ => continue,
        };
        if pos.is_valid() {
            positions.insert((pos.line, pos.column));
        }
    }
    positions
}

// ── Entry point ───────────────────────────────────────────────────────────────

pub fn provide(doc: Option<&Document>) -> Option<SemanticTokensResult> {
    let result = panic::catch_unwind(panic::AssertUnwindSafe(|| provide_inner(doc)));
    match result {
        Ok(r) => r,
        Err(payload) => {
            let msg = payload.downcast_ref::<String>().cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_else(|| "unknown panic".to_string());
            tracing::error!("semantic_tokens panicked: {}", msg);
            None
        }
    }
}

fn provide_inner(doc: Option<&Document>) -> Option<SemanticTokensResult> {
    let doc = doc?;

    // ── Semantic annotations from symbol table ────────────────────────────────
    let enum_names: HashSet<String> = doc
        .semantic_result.as_ref()
        .and_then(|sr| sr.symbol_table.as_ref())
        .map(|st| st.enums.keys().cloned().collect())
        .unwrap_or_default();

    let func_names: HashSet<String> = doc
        .semantic_result.as_ref()
        .and_then(|sr| sr.symbol_table.as_ref())
        .map(|st| st.functions.keys().cloned().collect())
        .unwrap_or_default();

    // ── Semantic annotations from AST ─────────────────────────────────────────
    let table_path_positions = build_table_path_positions(doc);

    // ── Type index from semantic analysis ─────────────────────────────────────
    // Maps identifier names → DataType.  Used to decide whether the token
    // before a `.` has a known type, which gates instance-method coloring.
    let empty_type_index: HashMap<String, DataType> = HashMap::new();
    let type_index: &HashMap<String, DataType> = doc
        .semantic_result.as_ref()
        .and_then(|sr| sr.type_index.as_ref())
        .unwrap_or(&empty_type_index);

    let data = encode_tokens(doc, &enum_names, &func_names, &table_path_positions, type_index);
    Some(SemanticTokensResult::Tokens(SemanticTokens {
        result_id: None,
        data,
    }))
}

// ── Stateful classifier ───────────────────────────────────────────────────────

struct ClassifierState<'a> {
    // ── Enum body tracking ────────────────────────────────────────────────────
    in_enum_body:      bool,
    enum_brace_depth:  i32,
    seen_enum_name:    bool,

    // ── QuickFunc declaration tracking ────────────────────────────────────────
    next_is_func_name: bool,
    in_param_list:     bool,
    param_paren_depth: i32,

    // ── Import alias tracking ─────────────────────────────────────────────────
    next_is_alias: bool,

    // ── DLM dot tracking ──────────────────────────────────────────────────────
    dlm_dot_seen: bool,

    // ── Call-site detection ───────────────────────────────────────────────────
    is_call_site: bool,

    // ── Enum access dot tracking ──────────────────────────────────────────────
    next_is_enum_type: bool,
    next_is_enum_dot:  bool,
    prev_was_enum_dot: bool,

    // ── Static / instance dot tracking ────────────────────────────────────────
    /// True when the current identifier is a known static object (Math, …).
    next_is_static_obj: bool,
    /// True when the preceding `.` followed a static object receiver.
    after_static_dot: bool,
    /// True when the preceding `.` was eligible for instance-method coloring:
    ///   - receiver had a known type (from type_index or is a literal/call result)
    ///   - and the pattern was not a table-path separator or enum access
    after_instance_dot: bool,

    // ── Table-path chain (AST-driven) ─────────────────────────────────────────
    /// Set when classify_identifier returns TT_FUNCTION for a position that is
    /// in `table_path_start_positions`.  Cleared by ControlFlowColon (`:`),
    /// DoubleColon (`::`), or a section-keyword token.  While set, every
    /// Identifier token in @DATA is also coloured TT_FUNCTION (yellow).
    in_table_path_chain: bool,

    // ── Type-aware dot gating ─────────────────────────────────────────────────
    /// Updated at the END of every advance() call to reflect whether the
    /// token just processed has a known type (literal, call result `()`,
    /// array result `[]`, or identifier present in `type_index`).
    /// Read by the `Symbol('.')` handler to decide `after_instance_dot`.
    prev_token_has_known_type: bool,

    // ── Semantic context ──────────────────────────────────────────────────────
    /// (1-based line, 1-based col) of every @DATA table/group-array path start.
    /// Built from the resolved AST, not from token lookahead.
    table_path_start_positions: &'a HashSet<(usize, usize)>,

    /// Identifier name → DataType from SemanticAnalysisResult::type_index.
    type_index: &'a HashMap<String, DataType>,

    enum_names: &'a HashSet<String>,
    func_names: &'a HashSet<String>,
}

impl<'a> ClassifierState<'a> {
    fn new(
        enum_names:               &'a HashSet<String>,
        func_names:               &'a HashSet<String>,
        table_path_start_positions: &'a HashSet<(usize, usize)>,
        type_index:               &'a HashMap<String, DataType>,
    ) -> Self {
        ClassifierState {
            in_enum_body:               false,
            enum_brace_depth:           0,
            seen_enum_name:             false,
            next_is_func_name:          false,
            in_param_list:              false,
            param_paren_depth:          0,
            next_is_alias:              false,
            dlm_dot_seen:               false,
            is_call_site:               false,
            next_is_enum_type:          false,
            next_is_enum_dot:           false,
            prev_was_enum_dot:          false,
            next_is_static_obj:         false,
            after_static_dot:           false,
            after_instance_dot:         false,
            in_table_path_chain:        false,
            prev_token_has_known_type:  false,
            table_path_start_positions,
            type_index,
            enum_names,
            func_names,
        }
    }

    fn advance(&mut self, token: &Token, tokens: &[Token], index: usize) {
        // ── 1. Reset per-token transient flags ────────────────────────────────
        self.is_call_site      = false;
        self.next_is_enum_type = false;

        // Clear in_table_path_chain at section-keyword boundaries so it can
        // never bleed across section transitions.
        if matches!(&token.token_type,
            TokenType::SectionConfig | TokenType::SectionImports | TokenType::SectionDLM
            | TokenType::SectionEnums | TokenType::SectionQuickFuncs
            | TokenType::SectionData  | TokenType::SectionSecurity)
        {
            self.in_table_path_chain       = false;
            self.after_static_dot          = false;
            self.after_instance_dot        = false;
            self.prev_token_has_known_type = false;
        }

        // ── 2. Clear dot/chain state on structural operator tokens ─────────────
        match &token.token_type {
            // Identifiers and dots are allowed to "carry" dot state forward.
            TokenType::Identifier(_) | TokenType::Symbol('.') => {}
            _ => {
                // Type-annotation tokens don't break dot state.
                match &token.token_type {
                    TokenType::Symbol('<') | TokenType::Symbol('>')
                    | TokenType::DataType(_) => {}
                    _ => {
                        match &token.token_type {
                            TokenType::ArithmeticOp(_)
                            | TokenType::ComparisonOp(_)
                            | TokenType::LogicalOp(_)
                            | TokenType::BitwiseOp(_)
                            | TokenType::ArithmeticAssignOp(_)
                            | TokenType::Arrow
                            | TokenType::SwitchCase
                            | TokenType::DoubleColon       // also ends a group-array path
                            | TokenType::ControlFlowColon  // also ends a table-property path
                            | TokenType::Symbol(';') => {
                                self.after_static_dot        = false;
                                self.after_instance_dot      = false;
                                self.next_is_static_obj      = false;
                                self.in_table_path_chain     = false; // ← clear table-path chain
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        // ── 3. Identifier-specific flag setup ─────────────────────────────────
        if let TokenType::Identifier(name) = &token.token_type {
            self.next_is_static_obj = STATIC_OBJECT_NAMES.contains(&name.as_str());

            let in_symbol_table = self.func_names.contains(name.as_str());
            let lookahead_paren = if in_symbol_table {
                true
            } else {
                is_followed_by_paren(tokens, index + 1)
            };
            self.is_call_site = lookahead_paren && !self.next_is_func_name;

            // Enum type detection: name is an enum AND next token is `.`
            if self.enum_names.contains(name.as_str()) {
                let has_dot = tokens.iter()
                    .skip(index + 1)
                    .take(2)
                    .any(|t| matches!(t.token_type, TokenType::Symbol('.')));
                if has_dot {
                    self.next_is_enum_type = true;
                    self.next_is_enum_dot  = true;
                } else {
                    self.next_is_enum_dot = false;
                }
            } else {
                self.next_is_enum_dot = false;
            }
        }

        // ── 4. Structural token state transitions ─────────────────────────────
        match &token.token_type {

            // ── Dot (.) ───────────────────────────────────────────────────────
            TokenType::Symbol('.') => {
                if self.in_table_path_chain {
                    // This dot is a table-path separator.  Never set instance/static
                    // method coloring — the token after it is the next path segment.
                    self.after_static_dot   = false;
                    self.after_instance_dot = false;

                } else if self.next_is_enum_dot {
                    // Enum field access: MyEnum.FIELD
                    self.prev_was_enum_dot  = true;
                    self.next_is_enum_dot   = false;
                    self.after_static_dot   = false;
                    self.after_instance_dot = false;

                } else {
                    self.prev_was_enum_dot = false;
                    self.after_static_dot  = self.next_is_static_obj;

                    if self.next_is_static_obj {
                        // Static receiver: Math.sqrt — after-dot is a static method.
                        self.after_instance_dot = false;
                    } else {
                        // Instance method / property access.
                        // Only colour after the dot when the receiver has a *known type*
                        // (literal, call result, or identifier present in the type index).
                        // This prevents table-path segments, unresolved enum names, and
                        // other unknown identifiers from triggering false method coloring.
                        self.after_instance_dot = self.prev_token_has_known_type;
                    }
                }

                // Always consume after reading.
                self.next_is_static_obj = false;
                self.dlm_dot_seen       = true;
            }

            // ── @ENUMS brace tracking ─────────────────────────────────────────
            TokenType::SectionEnums => {
                self.seen_enum_name = false;
            }
            TokenType::Symbol('{') if token.section == SectionId::Enums => {
                self.in_enum_body     = true;
                self.enum_brace_depth += 1;
            }
            TokenType::Symbol('}') if token.section == SectionId::Enums => {
                self.enum_brace_depth = (self.enum_brace_depth - 1).max(0);
                if self.enum_brace_depth == 0 {
                    self.in_enum_body   = false;
                    self.seen_enum_name = false;
                }
            }

            // ── QuickFunc declaration (~) ─────────────────────────────────────
            TokenType::Symbol('~') => {
                self.next_is_func_name = true;
                self.in_param_list     = false;
                self.param_paren_depth = 0;
            }
            TokenType::Symbol('(') if token.section == SectionId::QuickFuncs => {
                if self.next_is_func_name {
                    self.in_param_list     = true;
                    self.param_paren_depth = 1;
                } else if self.in_param_list {
                    self.param_paren_depth += 1;
                }
            }
            TokenType::Symbol(')') if token.section == SectionId::QuickFuncs => {
                if self.in_param_list {
                    self.param_paren_depth -= 1;
                    if self.param_paren_depth <= 0 {
                        self.in_param_list     = false;
                        self.param_paren_depth = 0;
                    }
                }
            }

            // ── Import alias ──────────────────────────────────────────────────
            TokenType::SectionImports => {
                self.next_is_alias = true;
            }
            TokenType::Keyword(kw)
                if *kw == "from" || *kw == "from_cloud" || *kw == "verify" =>
            {
                self.next_is_alias = false;
            }
            TokenType::String(_) | TokenType::StringSingle(_)
                if token.section == SectionId::Imports =>
            {
                self.next_is_alias = true;
            }

            // ── DLM section reset ─────────────────────────────────────────────
            TokenType::SectionDLM => {
                self.dlm_dot_seen = false;
            }
            TokenType::Symbol(',') if token.section == SectionId::Dlm => {
                self.dlm_dot_seen = false;
            }

            _ => {}
        }

        // ── 5. Update prev_token_has_known_type for the *next* dot's use ──────
        //
        // This is set at the END so the dot handler above reads the value from
        // the *previous* token's advance(), not the current token's.
        //
        // A token "has a known type" when:
        //   • It is a literal whose type is statically obvious (string, int, …)
        //   • It is a closing `)` (function / expression result) or `]` (array)
        //   • It is an identifier present in the semantic type_index
        //   • It is a known static-object name (Math, DateTime, …)
        //   • It is a known enum-type name (so `EnumName.FIELD` is not gated out)
        self.prev_token_has_known_type = match &token.token_type {
            // String literals
            TokenType::String(_)
            | TokenType::StringSingle(_)
            | TokenType::InterpolatedString(_) => true,
            // Colour literals
            TokenType::HexColor(_) => true,
            // Temporal literals
            TokenType::Date(_) | TokenType::Timestamp(_) => true,
            // Boolean literal
            TokenType::Bool(_) => true,
            // Numeric literals
            TokenType::Integer(_)
            | TokenType::Long(_)
            | TokenType::Float(_)
            | TokenType::Double(_)
            | TokenType::ScientificNotation(_)
            | TokenType::HexLiteral(_) => true,
            // Constructor results
            TokenType::TupleConstructor(_)
            | TokenType::BlobConstructor(_)
            | TokenType::RegexConstructor(_)
            | TokenType::PrefixedConstructor { .. } => true,
            // Closing brackets — result of a call or array literal
            TokenType::Symbol(']') | TokenType::Symbol(')') => true,
            // Identifiers — known only when present in the semantic type_index,
            // or when they are statically known (static objects, enum type names).
            TokenType::Identifier(name) => {
                self.type_index.contains_key(name.as_str())
                    || STATIC_OBJECT_NAMES.contains(&name.as_str())
                    || self.enum_names.contains(name.as_str())
            }
            // Everything else (operators, section keywords, dots, …) → unknown
            _ => false,
        };
    }

    /// Classify an `Identifier` token, consuming relevant state flags.
    ///
    /// Priority order:
    ///  0. DLM section (absolute override for module/subtype colouring)
    ///  1. Static-object receiver with lookahead dot  → TT_NAMESPACE
    ///  2. Control-flow keyword with lookahead colon  → TT_KEYWORD
    ///  3. Enum member after dot                      → TT_ENUM_MEMBER
    ///  4. Enum type name                             → TT_TYPE
    ///  5. After static dot                           → TT_FUNCTION+MOD_STATIC or TT_PROPERTY
    ///  6. After instance dot (type-gated)            → TT_METHOD or TT_PROPERTY
    ///  7. Regular QuickFunc call site                → TT_FUNCTION
    ///  8. Section-specific fallback                  → various
    fn classify_identifier(&mut self, token: &Token, tokens: &[Token], index: usize) -> (u32, u32) {

        // ── 0. DLM section — absolute priority ────────────────────────────────
        if token.section == SectionId::Dlm {
            let result = if self.dlm_dot_seen {
                (TT_DECORATOR, 0)
            } else {
                (TT_MACRO, MOD_DECLARATION)
            };
            self.dlm_dot_seen       = false;
            self.after_static_dot   = false;
            self.after_instance_dot = false;
            return result;
        }

        // ── 1. Static-object receiver (lookahead for '.') ─────────────────────
        if let TokenType::Identifier(name) = &token.token_type {
            if STATIC_OBJECT_NAMES.contains(&name.as_str()) {
                let next_is_dot = tokens.get(index + 1)
                    .map(|t| matches!(t.token_type, TokenType::Symbol('.')))
                    .unwrap_or(false);
                if next_is_dot {
                    return (TT_NAMESPACE, 0);
                }
            }
        }

        // ── 2. Control-flow keyword detection in QuickFuncs ───────────────────
        if token.section == SectionId::QuickFuncs {
            let next_is_colon = tokens.get(index + 1)
                .map(|t| matches!(t.token_type, TokenType::ControlFlowColon))
                .unwrap_or(false);
            if next_is_colon {
                return (TT_KEYWORD, 0);
            }
        }

        // ── 3. Enum member after dot (Status.ACTIVE) ──────────────────────────
        if self.prev_was_enum_dot {
            self.prev_was_enum_dot  = false;
            self.after_instance_dot = false;
            self.after_static_dot   = false;
            return (TT_ENUM_MEMBER, 0);
        }

        // ── 4. Enum type name (Status in Status.ACTIVE) ───────────────────────
        if self.next_is_enum_type {
            return (TT_TYPE, 0);
        }

        // ── 5. After static-object dot (Math.sqrt, DateTime.now) ─────────────
        if self.after_static_dot {
            let result = if self.is_call_site {
                (TT_FUNCTION, MOD_STATIC)
            } else {
                (TT_PROPERTY, MOD_STATIC)
            };
            self.after_static_dot   = false;
            self.after_instance_dot = false;
            return result;
        }

        // ── 6. After instance dot (type-gated) ────────────────────────────────
        // Only fires when `prev_token_has_known_type` was true at the dot.
        // This ensures `arr.get(0)` is coloured as a method call, while
        // `player.config:` (table path) and `AIType.FIELD` (enum, if not in
        // the symbol table) are NOT spuriously coloured.
        if self.after_instance_dot {
            let result = if self.is_call_site {
                (TT_METHOD, 0)
            } else {
                (TT_PROPERTY, 0)
            };
            self.after_instance_dot = false;
            return result;
        }

        // ── 7. Regular call site (direct QuickFunc / function call) ───────────
        if self.is_call_site {
            return (TT_FUNCTION, 0);
        }

        // ── 8. Section-specific fallback ──────────────────────────────────────
        match token.section {
            // @CONFIG keys — colour as properties.
            SectionId::Config => (TT_PROPERTY, 0),

            // @ENUMS: type declaration names and field values.
            SectionId::Enums => {
                if self.in_enum_body {
                    (TT_ENUM_MEMBER, MOD_DECLARATION)
                } else if !self.seen_enum_name {
                    self.seen_enum_name = true;
                    (TT_TYPE, MOD_DECLARATION)
                } else {
                    (TT_VARIABLE, 0)
                }
            }

            // @QUICKFUNCS: function names, parameters, local variables.
            SectionId::QuickFuncs => {
                if self.next_is_func_name {
                    self.next_is_func_name = false;
                    (TT_FUNCTION, MOD_DECLARATION)
                } else if self.in_param_list && self.param_paren_depth <= 1 {
                    (TT_PARAMETER, MOD_DECLARATION)
                } else {
                    (TT_VARIABLE, 0)
                }
            }

            // @IMPORTS: alias declarations.
            SectionId::Imports => {
                if self.next_is_alias {
                    self.next_is_alias = false;
                    (TT_NAMESPACE, MOD_DECLARATION)
                } else {
                    (TT_VARIABLE, 0)
                }
            }

            // @DATA: use AST-derived position set for table/group-array paths.
            //
            // If this identifier's (line, col) is in `table_path_start_positions`
            // (populated from DataEntry::TableProperty / GroupArray in the AST),
            // colour it yellow and start the chain so subsequent dotted segments
            // are also yellow.  If we are already inside such a chain, also yellow.
            SectionId::Data => {
                if self.in_table_path_chain {
                    // Subsequent segment of a table path (e.g. `config` in `player.config:`
                    // when the tokenizer emits it as a separate Identifier token).
                    return (TT_FUNCTION, 0);
                }
                let pos = (token.line, token.column);
                if self.table_path_start_positions.contains(&pos) {
                    // First segment of a table/group-array path.  Start the chain so
                    // the dot handler and subsequent segments inherit the yellow color.
                    self.in_table_path_chain = true;
                    return (TT_FUNCTION, 0);
                }
                (TT_VARIABLE, 0)
            }

            // @SECURITY: security block keys.
            SectionId::Security => (TT_PROPERTY, 0),

            // No section / unknown.
            _ => {
                if self.dlm_dot_seen {
                    self.dlm_dot_seen = false;
                    return (TT_DECORATOR, 0);
                }
                (TT_VARIABLE, 0)
            }
        }
    }
}

// ── Call-site lookahead ───────────────────────────────────────────────────────

fn is_followed_by_paren(tokens: &[Token], start: usize) -> bool {
    let mut i = start;
    let mut angle_depth: i32 = 0;

    while i < tokens.len() {
        match &tokens[i].token_type {
            TokenType::Symbol('<') => { angle_depth += 1; i += 1; }
            TokenType::Symbol('>') => { angle_depth -= 1; i += 1; }
            _ if angle_depth > 0  => { i += 1; }
            TokenType::Symbol('(') => return true,
            TokenType::Arrow => {
                i += 1;
                while i < tokens.len() {
                    match &tokens[i].token_type {
                        TokenType::Symbol('(') => return true,
                        TokenType::Identifier(_) | TokenType::Symbol(',') => { i += 1; }
                        _ => break,
                    }
                }
                break;
            }
            _ => break,
        }
        if i > start + 16 { break; }
    }
    false
}

// ── Encoder ───────────────────────────────────────────────────────────────────

fn encode_tokens(
    doc:                  &Document,
    enum_names:           &HashSet<String>,
    func_names:           &HashSet<String>,
    table_path_positions: &HashSet<(usize, usize)>,
    type_index:           &HashMap<String, DataType>,
) -> Vec<SemanticToken> {
    let mut data: Vec<SemanticToken> = Vec::with_capacity(doc.tokens.len());
    let mut prev_line: u32 = 0;
    let mut prev_col:  u32 = 0;
    let mut state = ClassifierState::new(
        enum_names, func_names, table_path_positions, type_index,
    );

    for (index, token) in doc.tokens.iter().enumerate() {
        state.advance(token, &doc.tokens, index);

        if let TokenType::InterpolatedString(content) = &token.token_type {
            emit_interpolated_tokens(token, content, &mut prev_line, &mut prev_col, &mut data);
            continue;
        }

        let (token_type, modifiers) = match classify(token, &mut state, &doc.tokens, index) {
            Some(t) => t,
            None    => continue,
        };

        let line   = token.line.saturating_sub(1) as u32;
        let col    = token.column.saturating_sub(1) as u32;
        let length = token_length(token) as u32;
        if length == 0 { continue; }

        push_raw(
            &mut data, &mut prev_line, &mut prev_col,
            line, col, length, token_type, modifiers,
        );
    }

    data
}

// ── Interpolated string ───────────────────────────────────────────────────────

fn emit_interpolated_tokens(
    token:     &Token,
    content:   &str,
    prev_line: &mut u32,
    prev_col:  &mut u32,
    data:      &mut Vec<SemanticToken>,
) {
    let base_line = token.line.saturating_sub(1) as u32;
    let base_col  = token.column.saturating_sub(1) as u32;

    if content.contains('\n') {
        push_raw(
            data, prev_line, prev_col,
            base_line, base_col, (content.len() + 3) as u32,
            TT_STRING, 0,
        );
        return;
    }

    let mut seg_start:   u32 = 0;
    let mut char_offset: u32 = 2;
    let mut in_brace          = false;
    let mut brace_start: u32  = 0;

    for ch in content.chars() {
        match ch {
            '{' if !in_brace => {
                let seg_len = char_offset - seg_start;
                if seg_len > 0 {
                    push_raw(data, prev_line, prev_col, base_line, base_col + seg_start, seg_len, TT_STRING, 0);
                }
                push_raw(data, prev_line, prev_col, base_line, base_col + char_offset, 1, TT_OPERATOR, 0);
                in_brace     = true;
                brace_start  = char_offset + 1;
                char_offset += 1;
            }
            '}' if in_brace => {
                let expr_len = char_offset - brace_start;
                if expr_len > 0 {
                    push_raw(data, prev_line, prev_col, base_line, base_col + brace_start, expr_len, TT_VARIABLE, 0);
                }
                push_raw(data, prev_line, prev_col, base_line, base_col + char_offset, 1, TT_OPERATOR, 0);
                in_brace     = false;
                seg_start    = char_offset + 1;
                char_offset += 1;
            }
            _ => { char_offset += 1; }
        }
    }

    if !in_brace {
        let seg_len = char_offset + 1 - seg_start;
        if seg_len > 0 {
            push_raw(data, prev_line, prev_col, base_line, base_col + seg_start, seg_len, TT_STRING, 0);
        }
    }
}

// ── Raw token emitter ─────────────────────────────────────────────────────────

fn push_raw(
    data:      &mut Vec<SemanticToken>,
    prev_line: &mut u32,
    prev_col:  &mut u32,
    line: u32, col: u32, len: u32, tt: u32, mods: u32,
) {
    if len == 0 { return; }
    if line < *prev_line || (line == *prev_line && col < *prev_col) { return; }
    let dl = line - *prev_line;
    let ds = if dl == 0 { col.saturating_sub(*prev_col) } else { col };
    data.push(SemanticToken {
        delta_line:             dl,
        delta_start:            ds,
        length:                 len,
        token_type:             tt,
        token_modifiers_bitset: mods,
    });
    *prev_line = line;
    *prev_col  = col;
}

// ── Per-token classification ──────────────────────────────────────────────────

fn classify(
    token: &Token,
    state: &mut ClassifierState<'_>,
    tokens: &[Token],
    index: usize,
) -> Option<(u32, u32)> {
    match &token.token_type {
        // ── Section keywords ──────────────────────────────────────────────────
        TokenType::SectionConfig
        | TokenType::SectionImports
        | TokenType::SectionDLM
        | TokenType::SectionEnums
        | TokenType::SectionQuickFuncs
        | TokenType::SectionData
        | TokenType::SectionSecurity     => Some((TT_KEYWORD, MOD_READONLY)),

        // ── Language keywords ─────────────────────────────────────────────────
        TokenType::Keyword(_)            => Some((TT_KEYWORD, 0)),
        TokenType::Bool(_)               => Some((TT_KEYWORD, MOD_READONLY)),
        TokenType::DataType(_)           => Some((TT_TYPE, 0)),

        // ── String literals ───────────────────────────────────────────────────
        TokenType::String(_)
        | TokenType::StringSingle(_)     => Some((TT_STRING, 0)),
        TokenType::InterpolatedString(_) => Some((TT_STRING, 0)),

        // ── Temporal values ───────────────────────────────────────────────────
        TokenType::Date(_)
        | TokenType::Timestamp(_)        => Some((TT_EVENT, 0)),

        // ── Numeric literals ──────────────────────────────────────────────────
        TokenType::Integer(_)
        | TokenType::Long(_)
        | TokenType::Float(_)
        | TokenType::Double(_)
        | TokenType::ScientificNotation(_) => Some((TT_NUMBER, 0)),
        TokenType::HexLiteral(_)           => Some((TT_NUMBER, 0)),
        TokenType::HexColor(_)             => Some((TT_NUMBER, MOD_READONLY)),

        // ── Operators ─────────────────────────────────────────────────────────
        TokenType::ArithmeticOp(_)
        | TokenType::ArithmeticAssignOp(_)
        | TokenType::ComparisonOp(_)
        | TokenType::LogicalOp(_)
        | TokenType::BitwiseOp(_)
        | TokenType::MultiCharSymbol(_)  => Some((TT_OPERATOR, 0)),

        TokenType::Arrow
        | TokenType::SwitchCase
        | TokenType::DoubleColon
        | TokenType::ControlFlowColon    => Some((TT_OPERATOR, 0)),

        TokenType::Symbol('~')           => Some((TT_OPERATOR, 0)),

        // ── Comments ──────────────────────────────────────────────────────────
        TokenType::Comment(_)            => Some((TT_COMMENT, 0)),

        // ── Enum access (pre-analysed by tokeniser) ───────────────────────────
        // EnumAccess covers the FULL `EnumName.FIELD` span.  Column is at the
        // start of the enum-type name, length covers name + '.' + field.
        TokenType::EnumAccess { .. }     => Some((TT_ENUM_MEMBER, 0)),

        // ── Table / group-array path segments (TablePath tokens) ─────────────
        // These tokens are emitted for the NON-FIRST segments of a dotted path,
        // e.g. `config` in `player.config:` or `name.d` in `player.name.d:`.
        // They are coloured yellow (TT_FUNCTION) to match the first-segment
        // Identifier which is coloured via the AST-based position set above.
        TokenType::TablePath(_)          => Some((TT_FUNCTION, 0)),

        // ── Pre-analysed static/builtin calls ─────────────────────────────────
        TokenType::StaticFunction { .. } if token.section == SectionId::Dlm
                                         => Some((TT_MACRO, 0)),
        TokenType::StaticFunction { .. } => Some((TT_FUNCTION, MOD_STATIC)),
        TokenType::DixFunction(_)        => Some((TT_FUNCTION, MOD_STATIC)),
        TokenType::BuiltinMethod(_)      => Some((TT_METHOD, 0)),

        // ── Prefixed constructors ─────────────────────────────────────────────
        TokenType::RegexConstructor(_)   => Some((TT_REGEXP, 0)),
        TokenType::BlobConstructor(_)
        | TokenType::TupleConstructor(_)
        | TokenType::PrefixedConstructor { .. } => Some((TT_KEYWORD, 0)),

        // ── Object / config access paths ──────────────────────────────────────
        TokenType::ObjectAccess(_) => {
            if token.section == SectionId::Dlm {
                Some((TT_MACRO, 0))
            } else {
                Some((TT_PROPERTY, 0))
            }
        }

        // ── Plain identifiers — full stateful classification ───────────────────
        TokenType::Identifier(_) => Some(state.classify_identifier(token, tokens, index)),

        // ── Scope declarations (@QUICKFUNCS => ScopeA, ScopeB) ───────────────
        TokenType::ScopeDeclaration(_) => Some((TT_TYPE, 0)),

        // ── Config access paths ────────────────────────────────────────────────
        TokenType::ConfigAccess(_)     => Some((TT_PROPERTY, 0)),

        // ── Ignored / structural ───────────────────────────────────────────────
        TokenType::ParseContext(_)
        | TokenType::Symbol(_)
        | TokenType::EndOfFile
        | TokenType::Error(_)          => None,
    }
}

// ── Token source-text length ──────────────────────────────────────────────────

fn token_length(token: &Token) -> usize {
    match &token.token_type {
        TokenType::String(s)              => s.len() + 2,
        TokenType::StringSingle(s)        => s.len() + 2,
        TokenType::InterpolatedString(s)  => s.len() + 3,
        TokenType::HexColor(h)            => h.trim_start_matches('#').len() + 1,
        TokenType::Comment(c)             => c.len() + 2,
        TokenType::Long(l)                => format!("{}L", l).len(),
        TokenType::SectionConfig          =>  7,
        TokenType::SectionImports         =>  8,
        TokenType::SectionDLM             =>  4,
        TokenType::SectionEnums           =>  6,
        TokenType::SectionQuickFuncs      => 11,
        TokenType::SectionData            =>  5,
        TokenType::SectionSecurity        =>  9,
        TokenType::DoubleColon            =>  2,
        TokenType::Arrow                  =>  2,
        TokenType::SwitchCase             =>  2,
        TokenType::ControlFlowColon       =>  1,
        TokenType::Bool(b)                => if *b { 4 } else { 5 },
        TokenType::BlobConstructor(_)     =>  2,
        TokenType::RegexConstructor(_)    =>  2,
        TokenType::TupleConstructor(_)    =>  2,
        TokenType::EnumAccess { enum_name, value } => enum_name.len() + 1 + value.len(),
        TokenType::TablePath(s)           => s.len(),
        TokenType::ObjectAccess(parts)    => parts.join(".").len(),
        _ => {
            let v = token.get_token_value();
            if v.is_empty() { 1 } else { v.len() }
        }
    }
    }
