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
//!
//! The lowercase spellings (`int`, `string`, `bool`, and so on) are all lexer keywords, and
//! the dotted-pattern analyzer only accepts a plain `Identifier` after `.`, so
//! `Schema.int(...)` cannot be written in source. `Schema.Int(...)` lexes as
//! `Identifier("Schema") . Identifier("Int")`. A lowercase name is still recognised here and
//! reported with a "did you mean `Int`?" message instead of a bare syntax error.
//!
//! ## Why this is its own mini-parser and not `DataSectionParser`
//!
//! `@DATA`'s value grammar has no static-builtin-call arm
//! (`IdentifierPatternType::StaticMethodCall` is only handled inside QuickFuncs), so wrapping
//! it cannot parse `Schema.Int(...)`. The descriptor here is deliberately narrow, a fixed
//! `Schema.<Method>(...)` call with literal arguments, so no expression grammar is needed.
//!
//! ## What this parser does not do
//!
//! It stores each descriptor as written (`method` plus literal `arguments`) and never
//! evaluates it. Whether the method exists, whether its arguments fit, duplicate paths and
//! the checking against `@DATA` are all `schema_section_analyzer.rs`'s job, the same parse
//! and semantic split every other section uses.
//!
//! # Values are built in an arena
//!
//! The parser owns an [`Arena`]. Every literal argument is built there as a node, each field
//! is kept as a [`DocSchemaField`] record whose arguments are a run of value ids, and
//! [`SchemaSectionParser::parse_section`] exports the finished section once, at the end, as
//! the owned [`SchemaBlock`]. The public interface and the result are the same as before the
//! parser used an arena; `tests/schema_parser_tests.rs` pins them.

use crate::Compiler::AST::{Position, SchemaBlock, TablePath};
use crate::Compiler::AST::arena::{Arena, DocSchema, DocSchemaField, ValueId, ValueList, ValueNode};
use crate::Compiler::Core::{OperationalSettings, ErrorHandlingStrategy};
use crate::ErrorManager::{ErrorManager, ParseErrorType, DebugConfig};
use crate::Compiler::Core::Tokenizer::{Token, TokenType};
use crate::Compiler::Core::Tokenizer::token::SectionId;

/// Returned in place of a token when the position is past the end of the slice.
static EOF: Token = Token {
    token_type: TokenType::EndOfFile,
    line: 1,
    column: 1,
    section: SectionId::None,
};

const MAX_ITERATIONS_PER_TOKEN: usize = 3;
const ABSOLUTE_MAX_ITERATIONS: usize = 500_000;
const MAX_STUCK_COUNT: usize = 3;

/// Parser for the `@SCHEMA(...)` section.
///
/// Create it over the section's tokens and call [`parse_section`](Self::parse_section).
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
    /// First syntax error reported, with its position. See [`first_error`](Self::first_error).
    first_error: Option<String>,
    /// Every literal argument of the section is built here; `parse_section` exports the
    /// finished section.
    arena: Arena,
}

impl<'a> SchemaSectionParser<'a> {
    /// Creates a parser that reports diagnostics to the process-wide shared [`ErrorManager`].
    ///
    /// Use [`new_with_error_manager`](Self::new_with_error_manager) to report to a specific one.
    pub fn new(tokens: &'a [Token], operational_settings: &'a OperationalSettings) -> Self {
        Self::new_with_error_manager(tokens, operational_settings, ErrorManager::get_shared_instance())
    }

    /// Creates a parser that reports diagnostics to `error_manager`.
    ///
    /// The parser stops after three times the token count in steps, capped at 500,000, so
    /// malformed input cannot make it loop forever.
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
            arena: Arena::new(),
        }
    }

    /// The first syntax error this parser reported, if any, as `"message (line L, column C)"`.
    ///
    /// `parse_section` returns `None` on a halt-strategy error, and `GeneralParser` turns a
    /// `None` section into an absent section instead of a compile error. For most sections
    /// that only loses diagnostics, but for `@SCHEMA` a typo would silently disable validation
    /// of the whole file, so `GeneralParser` reads this to fail loudly instead.
    pub fn first_error(&self) -> Option<&str> {
        self.first_error.as_deref()
    }

    /// Parses the section and returns it.
    ///
    /// Under the `halt` error strategy a parse error ends the section and the result is `None`.
    /// Under `continue` and `recover` the parser keeps going and returns the fields it could
    /// read. [`first_error`](Self::first_error) reports the first error under every strategy.
    pub fn parse_section(&mut self) -> Option<SchemaBlock> {
        let section_start_pos = Position::from_token(self.current());
        self.reset_parse_state();

        let mut fields: Vec<DocSchemaField> = Vec::with_capacity(8);

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
            return Some(self.finish_section(fields, section_start_pos));
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

        Some(self.finish_section(fields, section_start_pos))
    }

    /// Exports the section built in the arena as the owned `SchemaBlock`.
    fn finish_section(&self, fields: Vec<DocSchemaField>, position: Position) -> SchemaBlock {
        DocSchema { fields, position }.export(&self.arena)
    }

    // One field: `path = Schema.Method(args)`.

    fn parse_schema_field(&mut self) -> Option<DocSchemaField> {
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

        Some(DocSchemaField { path: TablePath::new(segments), method, arguments, position: field_start_pos })
    }

    /// `segment ("." segment)*` , the same dotted addressing `@DATA` uses.
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
    fn parse_descriptor_call(&mut self, path_text: &str) -> Option<(String, ValueList)> {
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
                // The lowercase spelling is a lexer keyword. Say so, and say what to write
                // instead, since this is the mistake people are most likely to make.
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

        let mut arguments: Vec<ValueId> = Vec::with_capacity(3);

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

        let arguments = self.arena.alloc_value_list(&arguments);
        Some((method, arguments))
    }

    /// One literal argument. Negative numbers arrive as a single signed
    /// numeric token from the lexer, so no unary-minus handling is needed.
    fn parse_argument(&mut self) -> Option<ValueId> {
        // `tokens` is a copy of the shared slice reference, so `token` does not borrow `self`
        // and the arena can be borrowed mutably while the token is still read.
        let tokens: &'a [Token] = self.tokens;
        let token: &'a Token = tokens.get(self.position).unwrap_or(&EOF);
        let pos = Position::from_token(token);
        let arena = &mut self.arena;

        let node = match &token.token_type {
            TokenType::String(s)             => Some(ValueNode::String  { value: arena.alloc_str(s), position: pos }),
            TokenType::StringSingle(s)       => Some(ValueNode::String  { value: arena.alloc_str(s), position: pos }),
            TokenType::Integer(i)            => Some(ValueNode::Integer { value: *i, position: pos }),
            TokenType::Long(i)               => Some(ValueNode::Long    { value: *i, position: pos }),
            TokenType::Float(fl)             => Some(ValueNode::Float   { value: *fl, position: pos }),
            TokenType::Double(d)             => Some(ValueNode::Double  { value: *d, position: pos }),
            TokenType::ScientificNotation(d) => Some(ValueNode::ScientificNotation { value: *d, position: pos }),
            TokenType::Bool(b)               => Some(ValueNode::Boolean { value: *b, position: pos }),
            TokenType::Keyword(k) if *k == "true"  => Some(ValueNode::Boolean { value: true,  position: pos }),
            TokenType::Keyword(k) if *k == "false" => Some(ValueNode::Boolean { value: false, position: pos }),
            _ => None,
        };

        let id = node.map(|n| self.arena.alloc_value(n));
        if id.is_some() { self.advance(); }
        id
    }

    // Error handling, recovery and cursor helpers, the same shape as in raw_section_parser.rs.

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

    fn partial_or_none(&self, start_pos: Position, fields: Vec<DocSchemaField>) -> Option<SchemaBlock> {
        if self.operational_settings.error_handling_strategy == ErrorHandlingStrategy::Halt {
            None
        } else {
            Some(self.finish_section(fields, start_pos))
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
