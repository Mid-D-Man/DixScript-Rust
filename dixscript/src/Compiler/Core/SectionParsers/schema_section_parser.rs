// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/compiler.md, section "Compiler/Core/SectionParsers/schema_section_parser.rs"
// ============================================================================
//! Parser for the `@SCHEMA(...)` section.
//!
//! ```text
//! SchemaSection ::= "@SCHEMA(" SchemaField* ")"
//! SchemaField   ::= FieldPath "=" "Schema" "." Method "(" ArgList? ")" ","?
//! FieldPath     ::= PathSegment ("." PathSegment)*
//! PathSegment   ::= Identifier | Keyword
//! Method        ::= Identifier            (capitalised: Int, Long, Float, Double,
//!                                          String, Bool, Array, Enum, Object,
//!                                          Tuple, Date, Timestamp, Hex, Blob, Regex)
//! ArgList       ::= Literal ("," Literal)*
//! Literal       ::= Boolean | Integer | Long | Float | Double
//!                 | ScientificNotation | StringLiteral
//! ```
//!
//! ## Why the method names are capitalised
//! The lowercase spellings (`int`, `string`, `bool`, …) are all lexer
//! keywords, and the dotted-pattern analyzer only accepts a plain
//! `Identifier` after `.` — so `Schema.int(...)` can't be written in source.
//! `Schema.Int(...)` lexes as `Identifier("Schema") . Identifier("Int")`.
//! A lowercase name is still recognised here and reported with a
//! "did you mean `Int`?" message rather than a bare syntax error.
//!
//! ## Why this is its own mini-parser and not `DataSectionParser`
//! `@DATA`'s value grammar has no static-builtin-call arm
//! (`IdentifierPatternType::StaticMethodCall` is only handled inside
//! QuickFuncs), so wrapping it can't parse `Schema.Int(...)`. The
//! descriptor here is deliberately narrow — a fixed `Schema.<Method>(...)`
//! call with literal arguments — so no expression grammar is needed.
//!
//! ## What this parser does NOT do
//! It stores each descriptor as written (`method` + literal `arguments`)
//! and never evaluates it. Whether the method exists, whether its
//! arguments fit, duplicate paths, and the actual checking against
//! `@DATA` are all `schema_section_analyzer.rs`'s job — the same
//! parse/semantic split every other section uses.

use crate::Compiler::AST::{Position, SchemaBlock, SchemaField, TablePath, Value};
use crate::Compiler::Core::{OperationalSettings, ErrorHandlingStrategy};
use crate::ErrorManager::{ErrorManager, ParseErrorType, DebugConfig};
use crate::Compiler::Core::Tokenizer::{Token, TokenType};
use crate::Compiler::Core::Tokenizer::token::SectionId;

const MAX_ITERATIONS_PER_TOKEN: usize = 3;
const ABSOLUTE_MAX_ITERATIONS: usize = 500_000;
const MAX_STUCK_COUNT: usize = 3;

pub struct SchemaSectionParser<'a> {
    tokens: &'a [Token],
    operational_settings: &'a OperationalSettings,
    error_manager: ErrorManager,
    debug_config: DebugConfig,
    position: usize,
    last_position: usize,
    stuck_count: usize,
    iteration_count: usize,
    max_iterations: usize,
    has_encountered_errors: bool,
    /// First syntax error reported, with its position. `GeneralParser` reads
    /// this to refuse a malformed `@SCHEMA` outright under the Halt strategy
    /// — see `first_error`.
    first_error: Option<String>,
}

impl<'a> SchemaSectionParser<'a> {
    pub fn new(tokens: &'a [Token], operational_settings: &'a OperationalSettings) -> Self {
        Self::new_with_error_manager(tokens, operational_settings, ErrorManager::get_shared_instance())
    }

    pub fn new_with_error_manager(
        tokens: &'a [Token],
        operational_settings: &'a OperationalSettings,
        error_manager: ErrorManager,
    ) -> Self {
        let debug_config = DebugConfig::from_debug_mode(operational_settings.debug_mode);

        let dynamic_limit  = tokens.len() * MAX_ITERATIONS_PER_TOKEN;
        let max_iterations = dynamic_limit.min(ABSOLUTE_MAX_ITERATIONS);

        if debug_config.is_enabled {
            error_manager.log_debug(&format!(
                "SCHEMA parser: {} tokens, strategy: {:?}",
                tokens.len(),
                operational_settings.error_handling_strategy
            ));
        }

        SchemaSectionParser {
            tokens,
            operational_settings,
            error_manager,
            debug_config,
            position: 0,
            last_position: usize::MAX,
            stuck_count: 0,
            iteration_count: 0,
            max_iterations,
            has_encountered_errors: false,
            first_error: None,
        }
    }

    /// The first syntax error this parser reported, if any (`"message (line L, column C)"`).
    ///
    /// Why this exists: `parse_section` returns `None` on a Halt-strategy
    /// error, and `GeneralParser` turns a `None` section into an absent
    /// section — not a compile error. For most sections that is a
    /// diagnostics-only loss, but for `@SCHEMA` it would mean a typo
    /// silently *disables validation of the whole file*. `GeneralParser`
    /// uses this to fail loudly instead.
    pub fn first_error(&self) -> Option<&str> {
        self.first_error.as_deref()
    }

    pub fn parse_section(&mut self) -> Option<SchemaBlock> {
        let section_start_pos = Position::from_token(self.current());
        self.reset_parse_state();

        let mut fields: Vec<SchemaField> = Vec::with_capacity(8);

        if !self.match_and_consume_symbol('(') {
            let current = self.current().clone();
            self.report_error(ParseErrorType::MissingToken, "Expected '(' to start SCHEMA section", &current);
            if self.should_halt_section() {
                return self.partial_or_none(section_start_pos, fields);
            }
            if !self.recover_to_symbol('(', 10) {
                return self.partial_or_none(section_start_pos, fields);
            }
        }

        if self.is_current_symbol(')') {
            self.advance();
            return Some(SchemaBlock::new(fields, section_start_pos));
        }

        while !self.is_at_end() && !self.is_current_symbol(')') && !self.should_terminate_loop() {
            self.track_progress();

            if self.is_stuck() {
                if !self.force_advance() { break; }
                continue;
            }

            // Commas between fields are optional, exactly as between @DATA entries.
            if self.is_current_symbol(',') {
                self.advance();
                continue;
            }

            match self.parse_schema_field() {
                Some(field) => fields.push(field),
                None => {
                    if self.should_halt_section() {
                        return self.partial_or_none(section_start_pos, fields);
                    }
                    if self.operational_settings.error_handling_strategy == ErrorHandlingStrategy::Recover {
                        if !self.recover_to_field_boundary() { self.ensure_progress(); }
                    } else {
                        self.ensure_progress();
                    }
                }
            }
        }

        if !self.match_and_consume_symbol(')') {
            let current = self.current().clone();
            self.report_error(ParseErrorType::MissingToken, "Expected ')' to close SCHEMA section", &current);
            if self.should_halt_section() {
                return self.partial_or_none(section_start_pos, fields);
            }
        }

        if self.debug_config.is_enabled {
            self.error_manager.log_debug(&format!(
                "SCHEMA section done: fields={}, errors={}",
                fields.len(), self.has_encountered_errors
            ));
        }

        Some(SchemaBlock::new(fields, section_start_pos))
    }

    // ═════════════════════════════════════════════════════════════════════
    // One field:  path = Schema.Method(args)
    // ═════════════════════════════════════════════════════════════════════

    fn parse_schema_field(&mut self) -> Option<SchemaField> {
        let field_start_pos = Position::from_token(self.current());

        let segments = self.parse_field_path()?;
        let path_text = segments.join(".");

        if !self.match_and_consume_symbol('=') {
            let current = self.current().clone();
            let msg = format!("Expected '=' after schema path '{}'", path_text);
            self.report_error(ParseErrorType::MissingToken, &msg, &current);
            if self.should_halt_section() { return None; }
            if self.operational_settings.error_handling_strategy == ErrorHandlingStrategy::Recover {
                if !self.recover_to_symbol('=', 10) { return None; }
            } else {
                return None;
            }
        }

        let (method, arguments) = self.parse_descriptor_call(&path_text)?;

        Some(SchemaField::new(TablePath::new(segments), method, arguments, field_start_pos))
    }

    /// `segment ("." segment)*` — the same dotted addressing `@DATA` uses.
    fn parse_field_path(&mut self) -> Option<Vec<String>> {
        let mut segments: Vec<String> = Vec::with_capacity(3);

        let first = self.parse_path_segment(
            "Expected a @DATA path (e.g. 'name' or 'table.key') at the start of a SCHEMA field",
        )?;
        segments.push(first);

        while self.is_current_symbol('.') {
            self.advance();
            let seg = self.parse_path_segment("Expected an identifier after '.' in a SCHEMA path")?;
            segments.push(seg);
        }

        Some(segments)
    }

    /// Path segments accept keywords as well as plain identifiers: a @DATA
    /// property may legitimately be called `string` or `date`, and this
    /// position is unambiguous.
    fn parse_path_segment(&mut self, context: &str) -> Option<String> {
        match &self.current().token_type {
            TokenType::Identifier(id) => { let s = id.clone(); self.advance(); Some(s) }
            TokenType::Keyword(k)     => { let s = k.to_string(); self.advance(); Some(s) }
            _ => {
                let current = self.current().clone();
                self.report_error(ParseErrorType::UnexpectedToken, context, &current);
                None
            }
        }
    }

    /// `Schema "." Method "(" ArgList? ")"`
    fn parse_descriptor_call(&mut self, path_text: &str) -> Option<(String, Vec<Value>)> {
        let is_schema = matches!(&self.current().token_type, TokenType::Identifier(id) if id == "Schema");
        if !is_schema {
            let current = self.current().clone();
            let msg = format!(
                "Expected 'Schema.<Type>(required, ...)' for '{}', found {}",
                path_text, current.get_token_value()
            );
            self.report_error(ParseErrorType::UnexpectedToken, &msg, &current);
            return None;
        }
        self.advance();

        if !self.match_and_consume_symbol('.') {
            let current = self.current().clone();
            let msg = format!("Expected '.' after 'Schema' in the descriptor for '{}'", path_text);
            self.report_error(ParseErrorType::MissingToken, &msg, &current);
            return None;
        }

        let method = match &self.current().token_type {
            TokenType::Identifier(id) => { let m = id.clone(); self.advance(); m }
            TokenType::Keyword(k) => {
                // The lowercase spelling is a lexer keyword. Say so, and say
                // what to write instead — this is the one mistake people
                // will make.
                let lower = k.to_string();
                let current = self.current().clone();
                let msg = format!(
                    "Schema type names are capitalised: write 'Schema.{}(...)', not 'Schema.{}(...)'",
                    capitalise(&lower), lower
                );
                self.report_error(ParseErrorType::UnexpectedToken, &msg, &current);
                return None;
            }
            _ => {
                let current = self.current().clone();
                let msg = format!(
                    "Expected a schema type name after 'Schema.' for '{}', found {}",
                    path_text, current.get_token_value()
                );
                self.report_error(ParseErrorType::UnexpectedToken, &msg, &current);
                return None;
            }
        };

        if !self.match_and_consume_symbol('(') {
            let current = self.current().clone();
            let msg = format!("Expected '(' after 'Schema.{}'", method);
            self.report_error(ParseErrorType::MissingToken, &msg, &current);
            return None;
        }

        let mut arguments: Vec<Value> = Vec::with_capacity(3);

        while !self.is_at_end() && !self.is_current_symbol(')') {
            match self.parse_argument() {
                Some(v) => arguments.push(v),
                None => {
                    let current = self.current().clone();
                    let msg = format!(
                        "Expected a literal argument (bool, number or string) in 'Schema.{}(...)', found {}",
                        method, current.get_token_value()
                    );
                    self.report_error(ParseErrorType::UnexpectedToken, &msg, &current);
                    return None;
                }
            }

            if self.is_current_symbol(',') {
                self.advance();
            } else if !self.is_current_symbol(')') {
                let current = self.current().clone();
                let msg = format!(
                    "Expected ',' or ')' in 'Schema.{}(...)' arguments, found {}",
                    method, current.get_token_value()
                );
                self.report_error(ParseErrorType::MissingToken, &msg, &current);
                return None;
            }
        }

        if !self.match_and_consume_symbol(')') {
            let current = self.current().clone();
            let msg = format!("Expected ')' to close 'Schema.{}(...)'", method);
            self.report_error(ParseErrorType::MissingToken, &msg, &current);
            return None;
        }

        Some((method, arguments))
    }

    /// One literal argument. Negative numbers arrive as a single signed
    /// numeric token from the lexer, so no unary-minus handling is needed.
    fn parse_argument(&mut self) -> Option<Value> {
        let pos = Position::from_token(self.current());
        let value = match &self.current().token_type {
            TokenType::String(s)             => Some(Value::String  { value: s.clone(), position: pos }),
            TokenType::StringSingle(s)       => Some(Value::String  { value: s.clone(), position: pos }),
            TokenType::Integer(i)            => Some(Value::Integer { value: *i, position: pos }),
            TokenType::Long(i)               => Some(Value::Long    { value: *i, position: pos }),
            TokenType::Float(fl)             => Some(Value::Float   { value: *fl, position: pos }),
            TokenType::Double(d)             => Some(Value::Double  { value: *d, position: pos }),
            TokenType::ScientificNotation(d) => Some(Value::ScientificNotation { value: *d, position: pos }),
            TokenType::Bool(b)               => Some(Value::Boolean { value: *b, position: pos }),
            TokenType::Keyword(k) if *k == "true"  => Some(Value::Boolean { value: true,  position: pos }),
            TokenType::Keyword(k) if *k == "false" => Some(Value::Boolean { value: false, position: pos }),
            _ => None,
        };
        if value.is_some() { self.advance(); }
        value
    }

    // ═════════════════════════════════════════════════════════════════════
    // Error handling / recovery / cursor helpers
    // (same shape as raw_section_parser.rs — see it for the reasoning)
    // ═════════════════════════════════════════════════════════════════════

    fn report_error(&mut self, error_type: ParseErrorType, message: &str, token: &Token) {
        self.has_encountered_errors = true;
        if self.first_error.is_none() {
            self.first_error = Some(format!("{} (line {}, column {})", message, token.line, token.column));
        }
        let source_line = self.reconstruct_source_line(token);
        self.error_manager.add_parse_error(
            error_type, message.to_string(), token.line, token.column, None, source_line,
        );
    }

    #[inline]
    fn should_halt_section(&self) -> bool {
        self.operational_settings.error_handling_strategy == ErrorHandlingStrategy::Halt
            && self.has_encountered_errors
    }

    fn partial_or_none(&self, start_pos: Position, fields: Vec<SchemaField>) -> Option<SchemaBlock> {
        if self.operational_settings.error_handling_strategy == ErrorHandlingStrategy::Halt {
            None
        } else {
            Some(SchemaBlock::new(fields, start_pos))
        }
    }

    fn recover_to_symbol(&mut self, symbol: char, max_steps: usize) -> bool {
        if self.operational_settings.error_handling_strategy != ErrorHandlingStrategy::Recover { return false; }
        for _ in 0..max_steps {
            if self.is_at_end() { return false; }
            if self.is_current_symbol(symbol) { self.advance(); return true; }
            self.advance();
        }
        false
    }

    /// Skip past a broken field. Tracks parenthesis depth so a `)` that
    /// belongs to the broken descriptor is consumed with it, while the
    /// section's own closing `)` is left in place.
    fn recover_to_field_boundary(&mut self) -> bool {
        if self.operational_settings.error_handling_strategy != ErrorHandlingStrategy::Recover { return false; }
        let mut depth: usize = 0;
        for _ in 0..100 {
            if self.is_at_end() { return true; }
            if self.is_current_symbol('(') {
                depth += 1;
                self.advance();
            } else if self.is_current_symbol(')') {
                if depth == 0 { return true; }
                depth -= 1;
                self.advance();
                if depth == 0 { return true; }
            } else if depth == 0 && self.is_current_symbol(',') {
                self.advance();
                return true;
            } else {
                self.advance();
            }
        }
        false
    }

    fn reconstruct_source_line(&self, token: &Token) -> Option<String> {
        let mut source = String::new();
        let mut col = 0usize;
        for t in self.tokens.iter().filter(|t| t.line == token.line) {
            while col < t.column { source.push(' '); col += 1; }
            let v = t.get_token_value();
            col += v.len();
            source.push_str(&v);
        }
        if source.is_empty() { None } else { Some(source) }
    }

    #[inline]
    fn current(&self) -> &Token {
        static EOF: Token = Token {
            token_type: TokenType::EndOfFile, line: 1, column: 1, section: SectionId::None,
        };
        self.tokens.get(self.position).unwrap_or(&EOF)
    }

    #[inline]
    fn is_at_end(&self) -> bool {
        self.position >= self.tokens.len() || matches!(self.current().token_type, TokenType::EndOfFile)
    }

    #[inline]
    fn advance(&mut self) {
        if self.position < self.tokens.len() { self.position += 1; }
    }

    #[inline]
    fn is_current_symbol(&self, symbol: char) -> bool {
        matches!(&self.current().token_type, TokenType::Symbol(s) if *s == symbol)
    }

    #[inline]
    fn match_and_consume_symbol(&mut self, symbol: char) -> bool {
        if self.is_current_symbol(symbol) { self.advance(); true } else { false }
    }

    fn reset_parse_state(&mut self) {
        self.last_position = usize::MAX;
        self.stuck_count = 0;
        self.iteration_count = 0;
        self.has_encountered_errors = false;
    }

    fn track_progress(&mut self) {
        self.iteration_count += 1;
        if self.position == self.last_position {
            self.stuck_count += 1;
        } else {
            self.last_position = self.position;
            self.stuck_count = 0;
        }
    }

    #[inline]
    fn is_stuck(&self) -> bool { self.stuck_count >= MAX_STUCK_COUNT }

    fn should_terminate_loop(&self) -> bool {
        if self.iteration_count >= self.max_iterations {
            self.error_manager.log_error(&format!(
                "SCHEMA parser exceeded {} iterations — possible infinite loop", self.max_iterations
            ));
            return true;
        }
        false
    }

    fn force_advance(&mut self) -> bool {
        if self.is_at_end() { return false; }
        self.advance();
        self.stuck_count = 0;
        true
    }

    #[inline]
    fn ensure_progress(&mut self) {
        if !self.is_at_end() { self.advance(); }
    }
}

/// `"int"` -> `"Int"`. Used only to build the "did you mean" hint for a
/// lowercase (keyword) type name.
fn capitalise(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}
