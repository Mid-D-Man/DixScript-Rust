// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/compiler.md, section "Compiler/Core/SectionParsers/raw_section_parser.rs"
// ============================================================================
//! Parser for the `@RAW(...)` section.
//!
//! ```text
//! RawSection    ::= "@RAW(" RawBlockEntry* ")"
//! RawBlockEntry ::= ("meta_data" | "using") "->" "{" RawFieldList? "}"
//!                 | "content" "->" RawContentToken
//! RawFieldList  ::= RawField ("," RawField)*
//! RawField      ::= Identifier "=" RawValue
//! RawValue      ::= StringLiteral | Integer | Long | Float | Double | Boolean | HexLiteral
//! ```
//!
//! `content`'s right-hand side is always exactly one `TokenType::RawContent` token, never
//! `{` or `}`, because the lexer has already consumed the whole delimited block by the time
//! this parser sees it. The field-list parsing of `meta_data` and `using` otherwise mirrors
//! `security_section_parser.rs`.
//!
//! The parser builds the most complete `RawBlock` it can from what is in the source. A
//! missing `content`, a missing `meta_data.id` or a duplicate block name is left as `None` or
//! absent here instead of failing the block, and `raw_section_analyzer.rs` reports it as a
//! semantic error. Cross-block checks (unique `id`, unique delimiter tag) are not attempted
//! here either, because this parser only ever sees one `@RAW(...)` occurrence at a time.
//!
//! # Values are built in an arena
//!
//! The parser owns an [`Arena`]. Every field value is built there as a node, the fields are
//! kept as [`DocRawField`] records that hold the value ids, and
//! [`RawSectionParser::parse_section`] exports the finished block once, at the end, as the
//! owned [`RawBlock`]. The public interface and the result are the same as before the parser
//! used an arena; `tests/raw_parser_tests.rs` pins them.

use crate::Compiler::AST::{RawBlock, RawContent, Position};
use crate::Compiler::AST::arena::{Arena, DocRawBlock, DocRawField, ValueId, ValueNode};
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

/// Parser for the `@RAW(...)` section.
///
/// Create it over the section's tokens and call [`parse_section`](Self::parse_section).
pub struct RawSectionParser<'a> {
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
    /// Every field value of the block is built here; `parse_section` exports the finished block.
    arena: Arena,
}

impl<'a> RawSectionParser<'a> {
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
                "RAW parser: {} tokens, strategy: {:?}",
                tokens.len(),
                operational_settings.error_handling_strategy
            ));
        }

        RawSectionParser {
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
            arena: Arena::new(),
        }
    }

    /// Parses the section and returns its block.
    ///
    /// Under the `halt` error strategy a parse error ends the section and the result is `None`.
    /// Under `continue` and `recover` the parser keeps going and returns the fields it could
    /// read, with a `Value::Error` standing in for a field whose value is missing.
    pub fn parse_section(&mut self) -> Option<RawBlock> {
        let section_start_pos = Position::from_token(self.current());
        self.reset_parse_state();

        let mut meta_data: Vec<DocRawField> = Vec::with_capacity(4);
        let mut using:     Vec<DocRawField> = Vec::with_capacity(4);
        let mut content:   Option<RawContent> = None;

        if !self.match_and_consume_symbol('(') {
            let current = self.current().clone();
            self.report_error(ParseErrorType::MissingToken, "Expected '(' to start RAW section", &current);
            if self.should_halt_section() {
                return self.partial_or_none(section_start_pos, meta_data, using, content);
            }
            if !self.recover_to_symbol('(', 10) {
                return self.partial_or_none(section_start_pos, meta_data, using, content);
            }
        }

        if self.is_current_symbol(')') {
            self.advance();
            return Some(self.finish_block(meta_data, using, content, section_start_pos));
        }

        while !self.is_at_end() && !self.is_current_symbol(')') && !self.should_terminate_loop() {
            self.track_progress();

            if self.is_stuck() {
                if !self.force_advance() { break; }
                continue;
            }

            // Optional commas between entries at the outer level.
            if self.is_current_symbol(',') {
                self.advance();
                continue;
            }

            let entry_start_pos = Position::from_token(self.current());
            let block_key = match self.parse_identifier_or_keyword(
                "Expected 'meta_data', 'using', or 'content' in RAW section",
            ) {
                Some(k) => k,
                None => {
                    if self.should_halt_section() {
                        return self.partial_or_none(section_start_pos, meta_data, using, content);
                    }
                    if self.operational_settings.error_handling_strategy == ErrorHandlingStrategy::Recover {
                        if !self.recover_to_entry_boundary() { self.ensure_progress(); }
                    } else {
                        self.ensure_progress();
                    }
                    continue;
                }
            };

            if !self.match_arrow() {
                let current = self.current().clone();
                let msg = format!(
                    "Expected '->' after '{}', found {}", block_key, current.get_token_value()
                );
                self.report_error(ParseErrorType::MissingToken, &msg, &current);
                if self.should_halt_section() {
                    return self.partial_or_none(section_start_pos, meta_data, using, content);
                }
                if self.operational_settings.error_handling_strategy == ErrorHandlingStrategy::Recover {
                    if !self.recover_to_arrow() { self.ensure_progress(); continue; }
                } else {
                    self.ensure_progress();
                    continue;
                }
            }

            match block_key.as_str() {
                "meta_data" => {
                    if !meta_data.is_empty() {
                        let current = self.current().clone();
                        self.report_error(
                            ParseErrorType::UnexpectedToken,
                            "Duplicate 'meta_data' block in the same @RAW section",
                            &current,
                        );
                    }
                    if let Some(fields) = self.parse_raw_field_block("meta_data") {
                        meta_data = fields;
                    } else if self.should_halt_section() {
                        return self.partial_or_none(section_start_pos, meta_data, using, content);
                    }
                }
                "using" => {
                    if !using.is_empty() {
                        let current = self.current().clone();
                        self.report_error(
                            ParseErrorType::UnexpectedToken,
                            "Duplicate 'using' block in the same @RAW section",
                            &current,
                        );
                    }
                    if let Some(fields) = self.parse_raw_field_block("using") {
                        using = fields;
                    } else if self.should_halt_section() {
                        return self.partial_or_none(section_start_pos, meta_data, using, content);
                    }
                }
                "content" => {
                    if content.is_some() {
                        let current = self.current().clone();
                        self.report_error(
                            ParseErrorType::UnexpectedToken,
                            "Duplicate 'content' block in the same @RAW section",
                            &current,
                        );
                    }
                    match self.parse_raw_content(entry_start_pos) {
                        Some(c) => content = Some(c),
                        None => {
                            if self.should_halt_section() {
                                return self.partial_or_none(section_start_pos, meta_data, using, content);
                            }
                        }
                    }
                }
                _ => {
                    let current = self.current().clone();
                    let msg = format!(
                        "Unrecognized RAW block key '{}' — expected 'meta_data', 'using', or 'content'",
                        block_key
                    );
                    self.report_error(ParseErrorType::UnexpectedToken, &msg, &current);
                    if self.should_halt_section() {
                        return self.partial_or_none(section_start_pos, meta_data, using, content);
                    }
                    if self.operational_settings.error_handling_strategy == ErrorHandlingStrategy::Recover {
                        if !self.recover_to_entry_boundary() { self.ensure_progress(); }
                    } else {
                        self.ensure_progress();
                    }
                }
            }
        }

        if !self.match_and_consume_symbol(')') {
            let current = self.current().clone();
            self.report_error(ParseErrorType::MissingToken, "Expected ')' to close RAW section", &current);
            if self.should_halt_section() {
                return self.partial_or_none(section_start_pos, meta_data, using, content);
            }
        }

        if self.debug_config.is_enabled {
            self.error_manager.log_debug(&format!(
                "RAW section done: meta_data={}, using={}, content={}, errors={}",
                meta_data.len(), using.len(), content.is_some(), self.has_encountered_errors
            ));
        }

        Some(self.finish_block(meta_data, using, content, section_start_pos))
    }

    /// Exports the block built in the arena as the owned `RawBlock`.
    fn finish_block(
        &self,
        meta_data: Vec<DocRawField>,
        using: Vec<DocRawField>,
        content: Option<RawContent>,
        position: Position,
    ) -> RawBlock {
        DocRawBlock { meta_data, using, content, position }.export(&self.arena)
    }

    /// Parses `{ key = value, key = value }` for `meta_data` and `using`. The field-list
    /// handling is the same as in `security_section_parser.rs`.
    fn parse_raw_field_block(&mut self, block_name: &str) -> Option<Vec<DocRawField>> {
        if !self.match_and_consume_symbol('{') {
            let current = self.current().clone();
            let msg = format!("Expected '{{' after '{} ->'", block_name);
            self.report_error(ParseErrorType::MissingToken, &msg, &current);
            if self.should_halt_section() { return None; }
            if self.operational_settings.error_handling_strategy == ErrorHandlingStrategy::Recover {
                if !self.recover_to_symbol('{', 10) { return None; }
            } else {
                return None;
            }
        }

        let mut fields = Vec::with_capacity(4);
        let mut need_comma = false;

        while !self.is_at_end() && !self.is_current_symbol('}') && !self.should_terminate_loop() {
            self.track_progress();

            if self.is_stuck() {
                if !self.force_advance() { break; }
                continue;
            }

            if need_comma {
                if self.is_current_symbol(',') {
                    self.advance();
                    if self.is_current_symbol('}') { break; }
                } else if !self.is_current_symbol('}') {
                    let current = self.current().clone();
                    let msg = format!(
                        "Expected ',' between fields in '{}' block, found {}",
                        block_name, current.get_token_value()
                    );
                    self.report_error(ParseErrorType::MissingToken, &msg, &current);
                    if self.should_halt_section() { return Some(fields); }
                    if !matches!(self.current().token_type, TokenType::Identifier(_) | TokenType::Keyword(_)) {
                        if self.operational_settings.error_handling_strategy == ErrorHandlingStrategy::Recover {
                            if !self.recover_in_fields() { self.ensure_progress(); }
                        } else {
                            self.ensure_progress();
                        }
                        continue;
                    }
                }
            }

            match self.parse_raw_field() {
                Some(field) => { fields.push(field); need_comma = true; }
                None => {
                    if self.should_halt_section() { return Some(fields); }
                    if self.operational_settings.error_handling_strategy == ErrorHandlingStrategy::Recover {
                        if !self.recover_in_fields() { self.ensure_progress(); }
                    } else {
                        self.ensure_progress();
                    }
                }
            }
        }

        if !self.match_and_consume_symbol('}') {
            let current = self.current().clone();
            let msg = format!("Expected '}}' to close '{}' block", block_name);
            self.report_error(ParseErrorType::MissingToken, &msg, &current);
        }

        Some(fields)
    }

    fn parse_raw_field(&mut self) -> Option<DocRawField> {
        let field_start_pos = Position::from_token(self.current());
        let key = self.parse_identifier_or_keyword("Expected field key identifier in RAW block")?;

        if !self.match_and_consume_symbol('=') {
            let current = self.current().clone();
            let msg = format!("Expected '=' after field key '{}'", key);
            self.report_error(ParseErrorType::MissingToken, &msg, &current);
            if self.should_halt_section() { return None; }
            if self.operational_settings.error_handling_strategy == ErrorHandlingStrategy::Recover {
                if !self.recover_to_symbol('=', 10) { return None; }
            } else {
                return None;
            }
        }

        let value = match self.parse_raw_value() {
            Some(v) => v,
            None => {
                let current = self.current().clone();
                let msg = format!(
                    "Expected value for field '{}', found {}", key, current.get_token_value()
                );
                self.report_error(ParseErrorType::UnexpectedToken, &msg, &current);
                if self.should_halt_section() { return None; }
                let message = self.arena.alloc_str(&format!("Missing value for key '{}'", key));
                self.arena.alloc_value(ValueNode::Error { message, position: field_start_pos })
            }
        };

        Some(DocRawField { key, value, position: field_start_pos })
    }

    fn parse_identifier_or_keyword(&mut self, context: &str) -> Option<String> {
        match &self.current().token_type {
            TokenType::Identifier(id) => { let k = id.clone(); self.advance(); Some(k) }
            TokenType::Keyword(k)     => { let k = k.to_string(); self.advance(); Some(k) }
            _ => {
                let current = self.current().clone();
                self.report_error(ParseErrorType::UnexpectedToken, context, &current);
                None
            }
        }
    }

    fn parse_raw_value(&mut self) -> Option<ValueId> {
        // `tokens` is a copy of the shared slice reference, so `token` does not borrow `self`
        // and the arena can be borrowed mutably while the token is still read.
        let tokens: &'a [Token] = self.tokens;
        let token: &'a Token = tokens.get(self.position).unwrap_or(&EOF);
        let pos = Position::from_token(token);
        let arena = &mut self.arena;

        let node = match &token.token_type {
            TokenType::String(s)       => Some(ValueNode::String  { value: arena.alloc_str(s), position: pos }),
            TokenType::StringSingle(s) => Some(ValueNode::String  { value: arena.alloc_str(s), position: pos }),
            TokenType::Integer(i)      => Some(ValueNode::Integer { value: *i, position: pos }),
            TokenType::Long(i)         => Some(ValueNode::Long    { value: *i, position: pos }),
            TokenType::Float(fl)       => Some(ValueNode::Float   { value: *fl, position: pos }),
            TokenType::Double(d)       => Some(ValueNode::Double  { value: *d, position: pos }),
            TokenType::Bool(b)         => Some(ValueNode::Boolean { value: *b, position: pos }),
            TokenType::HexColor(h)     => Some(ValueNode::HexColor { value: arena.alloc_str(h), position: pos }),
            TokenType::Keyword(k) if *k == "true"  => Some(ValueNode::Boolean { value: true,  position: pos }),
            TokenType::Keyword(k) if *k == "false" => Some(ValueNode::Boolean { value: false, position: pos }),
            TokenType::Keyword(k) if *k == "null"  => Some(ValueNode::Null { position: pos }),
            _ => None,
        };

        let id = node.map(|n| self.arena.alloc_value(n));
        if id.is_some() { self.advance(); }
        id
    }

    /// Parses the right-hand side of `content ->`, which is always one `RawContent` token (see
    /// the module docs).
    fn parse_raw_content(&mut self, entry_start_pos: Position) -> Option<RawContent> {
        match &self.current().token_type {
            TokenType::RawContent { tag, start, end } => {
                let content = RawContent::new(tag.clone(), *start, *end, entry_start_pos);
                self.advance();
                Some(content)
            }
            _ => {
                let current = self.current().clone();
                let msg = format!(
                    "Expected a raw content block after 'content ->', found {}",
                    current.get_token_value()
                );
                self.report_error(ParseErrorType::UnexpectedToken, &msg, &current);
                None
            }
        }
    }

    /// Consume a `->` token (`TokenType::SwitchCase`), with a two-symbol fallback.
    #[inline]
    fn match_arrow(&mut self) -> bool {
        if matches!(self.current().token_type, TokenType::SwitchCase) {
            self.advance();
            return true;
        }
        if matches!(self.current().token_type, TokenType::Symbol('-'))
            && self.position + 1 < self.tokens.len()
            && matches!(self.tokens[self.position + 1].token_type, TokenType::Symbol('>'))
        {
            self.advance();
            self.advance();
            return true;
        }
        false
    }

    fn report_error(&mut self, error_type: ParseErrorType, message: &str, token: &Token) {
        self.has_encountered_errors = true;
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

    fn partial_or_none(
        &self,
        start_pos: Position,
        meta_data: Vec<DocRawField>,
        using: Vec<DocRawField>,
        content: Option<RawContent>,
    ) -> Option<RawBlock> {
        if self.operational_settings.error_handling_strategy == ErrorHandlingStrategy::Halt {
            None
        } else {
            Some(self.finish_block(meta_data, using, content, start_pos))
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

    fn recover_to_arrow(&mut self) -> bool {
        if self.operational_settings.error_handling_strategy != ErrorHandlingStrategy::Recover { return false; }
        for _ in 0..10 {
            if self.is_at_end() { return false; }
            if self.match_arrow() { return true; }
            self.advance();
        }
        false
    }

    fn recover_to_entry_boundary(&mut self) -> bool {
        if self.operational_settings.error_handling_strategy != ErrorHandlingStrategy::Recover { return false; }
        for _ in 0..50 {
            if self.is_at_end() || self.is_current_symbol(')') { return true; }
            if matches!(self.current().token_type, TokenType::Identifier(ref id) if id == "meta_data" || id == "using" || id == "content") {
                return true;
            }
            self.advance();
        }
        false
    }

    fn recover_in_fields(&mut self) -> bool {
        if self.operational_settings.error_handling_strategy != ErrorHandlingStrategy::Recover { return false; }
        for _ in 0..50 {
            if self.is_at_end() || self.is_current_symbol(',') || self.is_current_symbol('}') { return true; }
            self.advance();
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
                "RAW parser exceeded {} iterations — possible infinite loop", self.max_iterations
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
