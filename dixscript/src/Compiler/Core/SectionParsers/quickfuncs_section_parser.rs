
// QuickFunctions Section Parser v1.0.0
//
// SPEC (BENF grammar):
//   QuickFuncsSection ::= "@QUICKFUNCS(" QuickFunc* ")"
//   QuickFunc         ::= "~" Identifier TypeAnnotation? ScopeDeclaration? FunctionSignature FunctionBody
//   TypeAnnotation    ::= "<" DataType ">"
//   ScopeDeclaration  ::= "=>" ScopeTarget ("," ScopeTarget)*
//   ScopeTarget       ::= "global" | QualifiedIdentifier
//   FunctionSignature ::= "(" ParameterList? ")"
//   ParameterList     ::= Parameter ("," Parameter)*
//   Parameter         ::= Identifier TypeAnnotation? ("=" DefaultValue)?
//   FunctionBody      ::= "{" Statement* ReturnStatement "}"
//
// All dotted identifier chains (A.B.C) become QualifiedIdentifier.
// The semantic analyzer resolves them (enum access, static call, property access, import, etc.).
// Error strategies: Halt = stop immediately; Continue = collect all errors; Recover = sync and resume.

use crate::Compiler::AST::{
    QuickFuncsSection, QuickFunction, QuickFuncParam, QuickFuncStatement, SwitchCase,
    Position, DataType, Expression, Value, ObjectProperty, DeclarationType,
};
use crate::Compiler::Core::{OperationalSettings, ErrorHandlingStrategy};
use crate::ErrorManager::{ErrorManager, ParseErrorType, DebugConfig};
use crate::Compiler::Core::Tokenizer::{Token, TokenType};
use crate::Compiler::Core::Tokenizer::token::SectionId;
use crate::Utilities::{Keywords, estimate_statements_count, estimate_properties_count};
use std::collections::HashMap;

const MAX_ITERATIONS_PER_TOKEN: usize = 3;
const ABSOLUTE_MAX_ITERATIONS: usize = 1_000_000;
const MAX_STUCK_COUNT: usize = 3;

lazy_static::lazy_static! {
    static ref OPERATOR_PRECEDENCE: HashMap<&'static str, (i32, bool)> = {
        let mut m = HashMap::new();
        m.insert("**",  (13, true));
        m.insert("*",   (12, false));
        m.insert("/",   (12, false));
        m.insert("%",   (12, false));
        m.insert("%%",  (12, false));
        m.insert("%&",  (12, false));
        m.insert("&%",  (12, false));
        m.insert("+",   (11, false));
        m.insert("-",   (11, false));
        m.insert("<<",  (10, false));
        m.insert(">>",  (10, false));
        m.insert(">",   (9,  false));
        m.insert("<",   (9,  false));
        m.insert(">=",  (9,  false));
        m.insert("<=",  (9,  false));
        m.insert("==",  (8,  false));
        m.insert("!=",  (8,  false));
        m.insert("&",   (7,  false));
        m.insert("^",   (6,  false));
        m.insert("|",   (5,  false));
        m.insert("&&",  (4,  false));
        m.insert("and", (4,  false));
        m.insert("||",  (3,  false));
        m.insert("or",  (3,  false));
        m
    };

    // Compiled once at first use, reused forever — zero per-call allocation.
    static ref INTERP_PLACEHOLDER_RE: regex::Regex =
        regex::Regex::new(r"\{([^}]+)\}").expect("INTERP_PLACEHOLDER_RE compile failed");

    static ref INTERP_METHOD_CALL_RE: regex::Regex =
        regex::Regex::new(r"^(\w+)\.(\w+)\(\)$").expect("INTERP_METHOD_CALL_RE compile failed");

    static ref INTERP_PROPERTY_RE: regex::Regex =
        regex::Regex::new(r"^(\w+)\.(\w+)$").expect("INTERP_PROPERTY_RE compile failed");
}

pub struct QuickFuncsSectionParser<'a> {
    tokens: &'a [Token],
    operational_settings: &'a OperationalSettings,
    error_manager: ErrorManager,
    debug_config: DebugConfig,
    position: usize,
    last_position: usize,
    stuck_count: usize,
    iteration_count: usize,
}

// =============================================================================
// Construction
// =============================================================================

impl<'a> QuickFuncsSectionParser<'a> {
    pub fn new(tokens: &'a [Token], operational_settings: &'a OperationalSettings) -> Self {
        let error_manager = ErrorManager::get_shared_instance();
        let debug_config = DebugConfig::from_debug_mode(operational_settings.debug_mode);

        if debug_config.is_enabled {
            error_manager.log_debug(&format!(
                "QuickFunctions parser: {} tokens, strategy: {:?}",
                tokens.len(),
                operational_settings.error_handling_strategy
            ));
        }

        QuickFuncsSectionParser {
            tokens,
            operational_settings,
            error_manager,
            debug_config,
            position: 0,
            last_position: usize::MAX,
            stuck_count: 0,
            iteration_count: 0,
        }
    }
    pub fn new_with_error_manager(
    tokens: &'a [Token],
    operational_settings: &'a OperationalSettings,
    error_manager: ErrorManager,
) -> Self {
    let debug_config   = DebugConfig::from_debug_mode(operational_settings.debug_mode);
    let dynamic_limit  = tokens.len() * MAX_ITERATIONS_PER_TOKEN;
    let max_iterations = dynamic_limit.min(ABSOLUTE_MAX_ITERATIONS);

    QuickFuncsSectionParser {
        tokens,
        operational_settings,
        error_manager,
        debug_config,
        position:             0,
        last_position:        usize::MAX,
        stuck_count:          0,
        iteration_count:      0,
        max_iterations,
        has_encountered_errors: false,
    }
}

    // =============================================================================
    // Main entry point
    // =============================================================================

    pub fn parse_section(&mut self) -> Option<QuickFuncsSection> {
        if self.debug_config.is_enabled {
            self.error_manager.log_debug("QuickFunctions: beginning section parse");
        }

        let section_start_token = self.current().clone();
        let section_start_pos = Position::from_token(&section_start_token);

        let mut functions = Vec::with_capacity(usize::max(2, self.tokens.len() / 50));

        if self.check_symbol('(') {
            self.advance();
        }

        let max_iterations = (self.tokens.len() * MAX_ITERATIONS_PER_TOKEN)
            .min(ABSOLUTE_MAX_ITERATIONS);

        while !self.is_at_end() && !self.check_symbol(')') && self.iteration_count < max_iterations {
            self.skip_whitespace();
            if self.is_at_end() || self.check_symbol(')') {
                break;
            }

            self.iteration_count += 1;
            let position_before = self.position;

            if self.check_symbol('~') {
                match self.parse_function() {
                    Some(func) => {
                        if self.debug_config.is_enabled {
                            self.error_manager.log_debug(&format!(
                                "QuickFunctions: parsed '{}'",
                                func.name
                            ));
                        }
                        functions.push(func);
                    }
                    None => {
                        if self.operational_settings.error_handling_strategy
                            == ErrorHandlingStrategy::Halt
                        {
                            return None;
                        }
                    }
                }
            } else {
                let current = self.current().clone();

                if matches!(current.token_type, TokenType::Symbol(';' | ',')) {
                    self.advance();
                    continue;
                }

                self.error_manager.add_parse_error(
                    ParseErrorType::UnexpectedToken,
                    format!(
                        "Expected '~' to start function definition, found {}",
                        current.get_token_value()
                    ),
                    current.line,
                    current.column,
                    Some("Each QuickFunction must begin with '~'".to_string()),
                    self.get_source_line(&current),
                );

                if self.operational_settings.error_handling_strategy
                    == ErrorHandlingStrategy::Halt
                {
                    return None;
                }

                self.advance();
            }

            if position_before == self.position && !self.is_at_end() {
                self.advance();
            }
        }

        if self.debug_config.is_enabled {
            self.error_manager.log_debug(&format!(
                "QuickFunctions: section complete, {} functions",
                functions.len()
            ));
        }

        Some(QuickFuncsSection::new(functions, section_start_pos))
    }

    // =============================================================================
    // Function structure parsing
    // =============================================================================

    /// Parse a complete function definition: `~name<type> => scope (params) { body }`
    fn parse_function(&mut self) -> Option<QuickFunction> {
        let function_start_pos = Position::from_token(self.current());

        if !self.expect_symbol('~') {
            return None;
        }

        let name_token = self.current().clone();
        let function_name = match &name_token.token_type {
            TokenType::Identifier(id) => id.clone(),
            _ => {
                self.error_manager.add_parse_error(
                    ParseErrorType::MissingToken,
                    "Expected function name after '~'".to_string(),
                    name_token.line,
                    name_token.column,
                    Some("Provide a valid identifier as the function name".to_string()),
                    self.get_source_line(&name_token),
                );
                return None;
            }
        };

        self.advance();
        self.skip_whitespace();

        if self.debug_config.is_enabled {
            self.error_manager.log_debug(&format!("QuickFunctions: parsing '{}'", function_name));
        }

        let return_type = if self.check_symbol('<') { self.parse_return_type() } else { None };
        self.skip_whitespace();

        let scope_list = if self.check_arrow() {
            self.parse_scope_declaration()
        } else {
            Some(vec!["global".to_string()])
        };
        self.skip_whitespace();

        let parameters = if self.check_symbol('(') {
            self.parse_parameters()
        } else {
            Vec::new()
        };
        self.skip_whitespace();

        if !self.check_symbol('{') {
            self.error_manager.add_parse_error(
                ParseErrorType::MissingToken,
                format!("Expected '{{' to open body of function '{}'", function_name),
                self.current().line,
                self.current().column,
                Some("Add '{' after the parameter list".to_string()),
                self.get_source_line(self.current()),
            );
            return None;
        }

        let body = self.parse_statement_block();

        if self.debug_config.is_enabled {
            self.error_manager.log_debug(&format!(
                "QuickFunctions: '{}' done — return={:?}, params={}, stmts={}",
                function_name,
                return_type,
                parameters.len(),
                body.len()
            ));
        }

        Some(QuickFunction::new(
            function_name,
            return_type,
            scope_list,
            parameters,
            body,
            function_start_pos,
        ))
    }

    fn parse_return_type(&mut self) -> Option<DataType> {
        self.parse_type_annotation()
    }

    /// Parse `=> global` or `=> data.users, data.posts`.
    fn parse_scope_declaration(&mut self) -> Option<Vec<String>> {
        let mut scopes = Vec::new();

        if !self.check_arrow() {
            return Some(scopes);
        }
        self.advance(); // consume '=>'

        loop {
            self.skip_whitespace();
            let token = self.current().clone();

            let scope_path = match &token.token_type {
                // Keyword: only "global" or contextually-allowed keywords are valid
                TokenType::Keyword(kw) => {
                    // *kw: &'static str — compare directly, no .as_str() needed
                    if *kw == "global" {
                        self.advance();
                        Some("global".to_string())
                    } else if Keywords::can_be_identifier_in_context(kw, "QUICKFUNCS") {
                        self.parse_dotted_path()
                    } else {
                        self.error_manager.add_parse_error(
                            ParseErrorType::UnexpectedToken,
                            format!("Cannot use reserved keyword '{}' in scope path", kw),
                            token.line,
                            token.column,
                            None,
                            self.get_source_line(&token),
                        );
                        None
                    }
                }
                TokenType::Identifier(_) => self.parse_dotted_path(),
                _ => {
                    self.error_manager.add_parse_error(
                        ParseErrorType::UnexpectedToken,
                        format!(
                            "Expected scope identifier or 'global' after '=>', found {}",
                            token.get_token_value()
                        ),
                        token.line,
                        token.column,
                        None,
                        self.get_source_line(&token),
                    );
                    None
                }
            };

            if let Some(path) = scope_path {
                scopes.push(path);
            } else {
                break;
            }

            self.skip_whitespace();
            if self.check_symbol(',') {
                self.advance();
            } else {
                break;
            }
        }

        if self.debug_config.is_verbose {
            self.error_manager.log_debug(&format!(
                "QuickFunctions: scopes = {:?}",
                scopes
            ));
        }

        Some(scopes)
    }

    /// Parse `(x<int>, y<float> = 42, z)`.
    fn parse_parameters(&mut self) -> Vec<QuickFuncParam> {
        let mut parameters = Vec::with_capacity(usize::max(2, self.tokens.len() / 100));

        if !self.expect_symbol('(') {
            return parameters;
        }
        self.skip_whitespace();

        if self.check_symbol(')') {
            self.advance();
            return parameters;
        }

        loop {
            self.skip_whitespace();
            let param_start_pos = Position::from_token(self.current());

            // -- parameter name -----------------------------------------------
            let param_name_opt = match &self.current().token_type {
                TokenType::Identifier(id) => {
                    let name = id.clone();
                    self.advance();
                    Some(name)
                }
                TokenType::Keyword(kw)
                if Keywords::can_be_identifier_in_context(kw, "QUICKFUNCS") =>
                    {
                        // kw: &&'static str — .to_string() gives owned String
                        let name = kw.to_string();
                        self.advance();
                        if self.debug_config.is_verbose {
                            self.error_manager.log_debug(&format!(
                                "QuickFunctions: keyword '{}' accepted as parameter name",
                                name
                            ));
                        }
                        Some(name)
                    }
                _ => {
                    let cur = self.current().clone();
                    self.error_manager.add_parse_error(
                        ParseErrorType::MissingToken,
                        "Expected parameter name".to_string(),
                        cur.line,
                        cur.column,
                        None,
                        self.get_source_line(&cur),
                    );
                    None
                }
            };

            if param_name_opt.is_none() {
                break;
            }
            let param_name = param_name_opt.unwrap();
            self.skip_whitespace();

            // -- optional type annotation <type> ------------------------------
            let mut param_type: Option<DataType> = None;
            let mut default_value: Option<Expression> = None;

            if self.check_symbol('<') {
                self.advance(); // consume '<'
                self.skip_whitespace();

                let type_token = self.current().clone();

                // Extract type string without OR-pattern (Keyword is &'static str, Identifier is String)
                let type_lower = match &type_token.token_type {
                    TokenType::Keyword(kw) => Some(kw.to_lowercase()),
                    TokenType::Identifier(id) => Some(id.to_lowercase()),
                    _ => None,
                };

                param_type = if let Some(ref s) = type_lower {
                    let dt = Self::str_to_data_type(s);
                    if dt.is_none() {
                        self.error_manager.add_parse_error(
                            ParseErrorType::InvalidType,
                            format!("Invalid parameter type '{}'", s),
                            type_token.line,
                            type_token.column,
                            None,
                            self.get_source_line(&type_token),
                        );
                    }
                    dt
                } else {
                    self.error_manager.add_parse_error(
                        ParseErrorType::InvalidType,
                        format!(
                            "Expected type keyword inside '<>', found {}",
                            type_token.get_token_value()
                        ),
                        type_token.line,
                        type_token.column,
                        None,
                        self.get_source_line(&type_token),
                    );
                    None
                };

                if type_lower.is_some() {
                    self.advance(); // consume the type token
                } else {
                    self.advance(); // skip invalid token
                }
                self.skip_whitespace();

                // Default value may appear inside <type = expr>
                if self.check_symbol('=') {
                    self.advance();
                    self.skip_whitespace();
                    default_value = Some(self.parse_expression(0));
                    self.skip_whitespace();
                }

                if !self.expect_symbol('>') {
                    break;
                }
            }

            self.skip_whitespace();

            // Default value outside the type annotation: name = expr
            if self.check_symbol('=') && default_value.is_none() {
                self.advance();
                self.skip_whitespace();
                default_value = Some(self.parse_expression(0));
                self.skip_whitespace();
            }

            if self.debug_config.is_verbose {
                self.error_manager.log_debug(&format!(
                    "QuickFunctions: param '{}' type={:?} has_default={}",
                    param_name,
                    param_type,
                    default_value.is_some()
                ));
            }

            parameters.push(QuickFuncParam::new(
                param_name,
                param_type,
                default_value,
                param_start_pos,
            ));

            if self.check_symbol(',') {
                self.advance();
            } else {
                break;
            }
        }

        if !self.expect_symbol(')') {
            return parameters;
        }

        parameters
    }

    // =============================================================================
    // Statement block
    // =============================================================================

    fn parse_statement_block(&mut self) -> Vec<QuickFuncStatement> {
        let mut statements = Vec::with_capacity(estimate_statements_count(self.tokens.len()));

        if !self.expect_symbol('{') {
            return statements;
        }

        let mut brace_depth: usize = 1;
        let max_iterations = (self.tokens.len() * MAX_ITERATIONS_PER_TOKEN)
            .min(ABSOLUTE_MAX_ITERATIONS);

        while !self.is_at_end() && brace_depth > 0 && self.iteration_count < max_iterations {
            self.skip_whitespace();

            if let TokenType::Symbol('}') = self.current().token_type {
                brace_depth -= 1;
                self.advance();
                if brace_depth == 0 {
                    break;
                }
                continue;
            }

            self.iteration_count += 1;
            let position_before = self.position;

            if let Some(stmt) = self.parse_statement() {
                statements.push(stmt);
            } else if self.operational_settings.error_handling_strategy
                == ErrorHandlingStrategy::Halt
            {
                break;
            }

            if position_before == self.position && !self.is_at_end() {
                if let TokenType::Symbol('}') = self.current().token_type {
                    brace_depth -= 1;
                    self.advance();
                    if brace_depth == 0 {
                        break;
                    }
                } else {
                    self.advance();
                }
            }
        }

        // Drain any remaining unmatched close braces at this depth
        if brace_depth > 0 && self.check_symbol('}') {
            self.advance();
        }

        statements
    }

    // =============================================================================
    // Statement parsing
    // =============================================================================

    fn parse_statement(&mut self) -> Option<QuickFuncStatement> {
        self.skip_whitespace();
        if self.is_at_end() {
            return None;
        }

        let token = self.current().clone();
        let stmt_pos = Position::from_token(&token);

        if let TokenType::Symbol('}') = token.token_type {
            return None;
        }

        // -- keyword-led statements -------------------------------------------
        if let TokenType::Keyword(kw) = &token.token_type {
            // kw: &&'static str — compare with *kw, no .as_str() needed
            if *kw == "return" {
                return self.parse_return_statement(stmt_pos);
            }
            if *kw == "if" {
                return self.parse_if_statement();
            }
            if *kw == "chk" {
                return self.parse_switch_statement();
            }
            if *kw == "log" {
                return self.parse_log_statement(stmt_pos);
            }
            if *kw == "let" {
                return Some(self.parse_variable_declaration(DeclarationType::Let, stmt_pos));
            }
            if *kw == "const" {
                return Some(self.parse_variable_declaration(DeclarationType::Const, stmt_pos));
            }

            // Contextually-allowed keyword used as a variable name
            if Keywords::can_be_identifier_in_context(kw, "QUICKFUNCS") {
                let var_name = kw.to_string(); // &&'static str -> String
                let saved = self.position;
                self.advance();
                self.skip_whitespace();

                match &self.current().token_type {
                    TokenType::Symbol('=') => {
                        self.advance();
                        self.skip_whitespace();
                        let expr = self.parse_expression(0);
                        self.skip_whitespace();
                        if self.check_symbol(';') { self.advance(); }
                        return Some(QuickFuncStatement::Assignment {
                            variable: var_name,
                            value: expr,
                            position: stmt_pos,
                        });
                    }
                    TokenType::ArithmeticAssignOp(op) => {
                        let operator = op.to_string(); // &&'static str -> String
                        self.advance();
                        self.skip_whitespace();
                        let expr = self.parse_expression(0);
                        self.skip_whitespace();
                        if self.check_symbol(';') { self.advance(); }
                        return Some(QuickFuncStatement::ArithmeticAssignment {
                            variable: var_name,
                            operator,
                            value: expr,
                            position: stmt_pos,
                        });
                    }
                    _ => {
                        self.position = saved;
                    }
                }
            }
        }

        // -- identifier-led: "log" as identifier, assignment, arith-assign ----
        if let TokenType::Identifier(id) = &token.token_type {
            if id.eq_ignore_ascii_case("log") {
                return self.parse_log_statement(stmt_pos);
            }

            let var_name = id.clone();
            let saved = self.position;
            self.advance();
            self.skip_whitespace();

            match &self.current().token_type {
                TokenType::Symbol('=') => {
                    self.advance();
                    self.skip_whitespace();
                    let expr = self.parse_expression(0);
                    self.skip_whitespace();
                    if self.check_symbol(';') { self.advance(); }
                    return Some(QuickFuncStatement::Assignment {
                        variable: var_name,
                        value: expr,
                        position: stmt_pos,
                    });
                }
                TokenType::ArithmeticAssignOp(op) => {
                    let operator = op.to_string(); // &&'static str -> String
                    self.advance();
                    self.skip_whitespace();
                    let expr = self.parse_expression(0);
                    self.skip_whitespace();
                    if self.check_symbol(';') { self.advance(); }
                    return Some(QuickFuncStatement::ArithmeticAssignment {
                        variable: var_name,
                        operator,
                        value: expr,
                        position: stmt_pos,
                    });
                }
                TokenType::BitwiseOp(op) if op.ends_with('=') => {
                    let operator = op.to_string(); // &&'static str -> String
                    self.advance();
                    self.skip_whitespace();
                    let expr = self.parse_expression(0);
                    self.skip_whitespace();
                    if self.check_symbol(';') { self.advance(); }
                    return Some(QuickFuncStatement::ArithmeticAssignment {
                        variable: var_name,
                        operator,
                        value: expr,
                        position: stmt_pos,
                    });
                }
                _ => {
                    self.position = saved;
                }
            }
        }

        // -- fallback: expression statement -----------------------------------
        let expr = self.parse_expression(0);
        self.skip_whitespace();
        if self.check_symbol(';') { self.advance(); }
        Some(QuickFuncStatement::ExpressionStatement {
            expression: expr,
            position: stmt_pos,
        })
    }

    fn parse_return_statement(&mut self, pos: Position) -> Option<QuickFuncStatement> {
        self.advance(); // consume 'return'
        self.skip_whitespace();

        let expr = if !self.check_symbol(';') && !self.check_symbol('}') {
            self.parse_expression(0)
        } else {
            Expression::Value {
                value: Value::Null { position: pos },
                position: pos,
            }
        };

        self.skip_whitespace();
        if self.check_symbol(';') { self.advance(); }

        Some(QuickFuncStatement::Return { value: expr, position: pos })
    }

    fn parse_log_statement(&mut self, position: Position) -> Option<QuickFuncStatement> {
        self.advance(); // consume 'log' (keyword or identifier)
        self.skip_whitespace();

        if !self.check_symbol(':') {
            let cur = self.current().clone();
            self.error_manager.add_parse_error(
                ParseErrorType::MissingToken,
                "Expected ':' after 'log'".to_string(),
                cur.line,
                cur.column,
                None,
                self.get_source_line(&cur),
            );
            return None;
        }
        self.advance();
        self.skip_whitespace();

        let expr = self.parse_expression(0);
        self.skip_whitespace();
        if self.check_symbol(';') { self.advance(); }

        Some(QuickFuncStatement::Log { value: expr, position })
    }

    /// Parse `let [mut] name[<type>] = expr` or `const name[<type>] = expr`.
    fn parse_variable_declaration(
        &mut self,
        decl_type: DeclarationType,
        start_pos: Position,
    ) -> QuickFuncStatement {
        self.advance(); // consume 'let' / 'const'
        self.skip_whitespace();

        // 'mut' is only valid after 'let'
        let is_mutable = if decl_type == DeclarationType::Let {
            if let TokenType::Keyword(kw) = &self.current().token_type {
                if *kw == "mut" {
                    self.advance();
                    self.skip_whitespace();
                    true
                } else {
                    false
                }
            } else {
                false
            }
        } else {
            // Detect and reject `const mut`
            if let TokenType::Keyword(kw) = &self.current().token_type {
                if *kw == "mut" {
                    let cur = self.current().clone();
                    self.error_manager.add_parse_error(
                        ParseErrorType::InvalidOperation,
                        "'const' declarations cannot be mutable — use 'let mut' instead".to_string(),
                        cur.line,
                        cur.column,
                        Some("Replace 'const' with 'let mut'".to_string()),
                        self.get_source_line(&cur),
                    );
                    self.advance();
                    self.skip_whitespace();
                }
            }
            false
        };

        // Variable name
        let var_name = match &self.current().token_type {
            TokenType::Identifier(id) => {
                let name = id.clone();
                self.advance();
                name
            }
            TokenType::Keyword(kw)
            if Keywords::can_be_identifier_in_context(kw, "QUICKFUNCS") =>
                {
                    let name = kw.to_string(); // &&'static str -> String
                    self.advance();
                    name
                }
            _ => {
                let cur = self.current().clone();
                self.error_manager.add_parse_error(
                    ParseErrorType::MissingToken,
                    format!(
                        "Expected variable name after '{}'",
                        if decl_type == DeclarationType::Let { "let" } else { "const" }
                    ),
                    cur.line,
                    cur.column,
                    None,
                    self.get_source_line(&cur),
                );
                // Error-recovery: emit a placeholder declaration so parsing continues.
                return QuickFuncStatement::ExpressionStatement {
                    expression: Expression::Value {
                        value: Value::Null { position: start_pos },
                        position: start_pos,
                    },
                    position: start_pos,
                };
            }
        };

        self.skip_whitespace();

        let var_type = if self.check_symbol('<') { self.parse_type_annotation() } else { None };
        self.skip_whitespace();

        if !self.check_symbol('=') {
            let cur = self.current().clone();
            self.error_manager.add_parse_error(
                ParseErrorType::MissingToken,
                format!("Expected '=' after variable declaration '{}'", var_name),
                cur.line,
                cur.column,
                None,
                self.get_source_line(&cur),
            );
            return QuickFuncStatement::ExpressionStatement {
                expression: Expression::Value {
                    value: Value::Null { position: start_pos },
                    position: start_pos,
                },
                position: start_pos,
            };
        }

        self.advance();
        self.skip_whitespace();

        let init_expr = self.parse_expression(0);
        self.skip_whitespace();
        if self.check_symbol(';') { self.advance(); }

        QuickFuncStatement::VariableDeclaration {
            declaration_type: decl_type,
            is_mutable,
            variable_name: var_name,
            data_type: var_type,
            value: init_expr,
            position: start_pos,
        }
    }

    // =============================================================================
    // Control flow — if / elif / else
    // =============================================================================

    fn parse_if_statement(&mut self) -> Option<QuickFuncStatement> {
        let if_pos = Position::from_token(self.current());
        self.advance(); // consume 'if'

        if !self.expect_symbol(':') {
            return Some(QuickFuncStatement::If {
                condition: Expression::Value {
                    value: Value::Boolean { value: false, position: if_pos },
                    position: if_pos,
                },
                then_branch: Vec::new(),
                else_branch: None,
                position: if_pos,
            });
        }

        self.skip_whitespace();
        let condition = self.parse_expression(0);
        self.skip_whitespace();

        // Support single-line `if: cond then stmt`
        let is_single_line = if let TokenType::Keyword(kw) = &self.current().token_type {
            *kw == "then" // &&'static str comparison — no .as_str() needed
        } else {
            false
        };

        let then_branch = if is_single_line {
            self.advance(); // consume 'then'
            self.skip_whitespace();
            if let Some(stmt) = self.parse_statement() { vec![stmt] } else { Vec::new() }
        } else {
            if !self.check_symbol('{') {
                let cur = self.current().clone();
                self.error_manager.add_parse_error(
                    ParseErrorType::MissingToken,
                    "Expected '{' or 'then' after if condition".to_string(),
                    cur.line,
                    cur.column,
                    None,
                    self.get_source_line(&cur),
                );
                return Some(QuickFuncStatement::If {
                    condition,
                    then_branch: Vec::new(),
                    else_branch: None,
                    position: if_pos,
                });
            }
            self.parse_statement_block()
        };

        self.skip_whitespace();

        // Collect elif branches
        let mut elif_chain: Vec<QuickFuncStatement> = Vec::new();

        loop {
            self.skip_whitespace();

            let is_elif = if let TokenType::Keyword(kw) = &self.current().token_type {
                *kw == "elif" // &&'static str — direct comparison
            } else {
                false
            };

            if !is_elif {
                break;
            }

            let elif_pos = Position::from_token(self.current());
            self.advance(); // consume 'elif'
            self.skip_whitespace();

            if !self.expect_symbol(':') {
                break;
            }
            self.skip_whitespace();

            let elif_cond = self.parse_expression(0);
            self.skip_whitespace();

            if !self.check_symbol('{') {
                let cur = self.current().clone();
                self.error_manager.add_parse_error(
                    ParseErrorType::MissingToken,
                    "Expected '{' after elif condition".to_string(),
                    cur.line,
                    cur.column,
                    None,
                    self.get_source_line(&cur),
                );
                break;
            }

            let elif_body = self.parse_statement_block();
            self.skip_whitespace();

            elif_chain.push(QuickFuncStatement::If {
                condition: elif_cond,
                then_branch: elif_body,
                else_branch: None,
                position: elif_pos,
            });
        }

        // Optional else branch
        let mut final_else: Option<Vec<QuickFuncStatement>> = None;

        let is_else = if let TokenType::Keyword(kw) = &self.current().token_type {
            *kw == "else" // &&'static str — direct comparison
        } else {
            false
        };

        if is_else {
            self.advance(); // consume 'else'
            self.skip_whitespace();

            if self.check_symbol('{') {
                final_else = Some(self.parse_statement_block());
            } else {
                let cur = self.current().clone();
                self.error_manager.add_parse_error(
                    ParseErrorType::MissingToken,
                    "Expected '{' after 'else'".to_string(),
                    cur.line,
                    cur.column,
                    None,
                    self.get_source_line(&cur),
                );
            }
        }

        // Build elif → else chain from bottom up
        let mut current_else = final_else;
        for elif in elif_chain.into_iter().rev() {
            if let QuickFuncStatement::If { condition, then_branch, position, .. } = elif {
                current_else = Some(vec![QuickFuncStatement::If {
                    condition,
                    then_branch,
                    else_branch: current_else,
                    position,
                }]);
            }
        }

        Some(QuickFuncStatement::If {
            condition,
            then_branch,
            else_branch: current_else,
            position: if_pos,
        })
    }
    // =============================================================================
    // Control flow — switch / chk
    // =============================================================================

    fn parse_switch_statement(&mut self) -> Option<QuickFuncStatement> {
        let switch_pos = Position::from_token(self.current());
        self.advance(); // consume 'chk'

        if !self.expect_symbol(':') {
            return Some(QuickFuncStatement::Switch {
                expression: Expression::Value {
                    value: Value::Null { position: switch_pos },
                    position: switch_pos,
                },
                cases: Vec::new(),
                default_case: None,
                position: switch_pos,
            });
        }

        self.skip_whitespace();
        let expr = self.parse_expression(0);
        self.skip_whitespace();

        if !self.expect_symbol('{') {
            return Some(QuickFuncStatement::Switch {
                expression: expr,
                cases: Vec::new(),
                default_case: None,
                position: switch_pos,
            });
        }

        let mut cases = Vec::new();
        let mut default_case: Option<SwitchCase> = None;

        while !self.is_at_end() && !self.check_symbol('}') {
            self.skip_whitespace();
            if self.check_symbol('}') {
                break;
            }

            let case_pos = Position::from_token(self.current());

            if !self.match_arrow() {
                let cur = self.current().clone();
                self.error_manager.add_parse_error(
                    ParseErrorType::UnexpectedToken,
                    "Expected '->' to begin switch case".to_string(),
                    cur.line,
                    cur.column,
                    Some("Each case must start with '->'".to_string()),
                    self.get_source_line(&cur),
                );
                self.advance();
                continue;
            }

            self.skip_whitespace();

            // Check for `miss` (default case)
            let is_miss = if let TokenType::Keyword(kw) = &self.current().token_type {
                *kw == "miss" // &&'static str — direct comparison
            } else {
                false
            };

            if is_miss {
                self.advance(); // consume 'miss'
                self.skip_whitespace();
                let stmts = self.parse_case_body();
                default_case = Some(SwitchCase::new(
                    Value::Null { position: case_pos },
                    stmts,
                    case_pos,
                ));
            } else {
                let case_val = self.parse_value();
                self.skip_whitespace();
                let stmts = self.parse_case_body();
                cases.push(SwitchCase::new(case_val, stmts, case_pos));
            }

            self.skip_whitespace();
        }

        if !self.expect_symbol('}') {
            return Some(QuickFuncStatement::Switch {
                expression: expr,
                cases: Vec::new(),
                default_case: None,
                position: switch_pos,
            });
        }

        Some(QuickFuncStatement::Switch {
            expression: expr,
            cases,
            default_case,
            position: switch_pos,
        })
    }

    fn parse_case_body(&mut self) -> Vec<QuickFuncStatement> {
        self.skip_whitespace();

        // `then` keyword — single statement
        let is_then = if let TokenType::Keyword(kw) = &self.current().token_type {
            *kw == "then" // &&'static str
        } else {
            false
        };

        if is_then {
            self.advance();
            self.skip_whitespace();
            return if let Some(stmt) = self.parse_statement() { vec![stmt] } else { Vec::new() };
        }

        // `=>` arrow — single statement
        if self.check_arrow() {
            self.advance();
            self.skip_whitespace();
            return if let Some(stmt) = self.parse_statement() { vec![stmt] } else { Vec::new() };
        }

        // block body
        if self.check_symbol('{') {
            return self.parse_statement_block();
        }

        let cur = self.current().clone();
        self.error_manager.add_parse_error(
            ParseErrorType::MissingToken,
            "Expected 'then', '=>', or '{' after switch case value".to_string(),
            cur.line,
            cur.column,
            None,
            self.get_source_line(&cur),
        );

        Vec::new()
    }

    // =============================================================================
    // Pratt expression parser
    // =============================================================================

    fn parse_expression(&mut self, min_precedence: i32) -> Expression {
        self.skip_whitespace();

        if self.is_at_end() {
            return Expression::Value {
                value: Value::Null { position: Position::UNKNOWN },
                position: Position::UNKNOWN,
            };
        }

        let mut left = self.parse_unary_or_primary();
        self.skip_whitespace();

        while !self.is_at_end() {
            let current_token = self.current().clone();

            // Ternary operator
            if let TokenType::Symbol('?') = current_token.token_type {
                let ternary_prec = 2;
                if ternary_prec < min_precedence {
                    break;
                }

                let ternary_pos = Position::from_token(&current_token);
                self.advance();
                self.skip_whitespace();

                let true_branch = self.parse_expression(2);
                self.skip_whitespace();

                if !self.check_symbol(':') {
                    let cur = self.current().clone();
                    self.error_manager.add_parse_error(
                        ParseErrorType::MissingToken,
                        "Expected ':' in ternary expression".to_string(),
                        cur.line,
                        cur.column,
                        None,
                        self.get_source_line(&cur),
                    );
                    return left;
                }

                self.advance();
                self.skip_whitespace();

                let false_branch = self.parse_expression(2);

                left = Expression::Conditional {
                    condition: Box::new(left),
                    true_value: Box::new(true_branch),
                    false_value: Box::new(false_branch),
                    position: ternary_pos,
                };
                self.skip_whitespace();
                continue;
            }

            let (op, prec, right_assoc) = match self.try_get_operator_precedence(&current_token) {
                Some(info) => info,
                None => break,
            };

            if prec < min_precedence {
                break;
            }

            let op_pos = Position::from_token(&current_token);
            self.advance();
            self.skip_whitespace();

            let next_min = if right_assoc { prec } else { prec + 1 };
            let right = self.parse_expression(next_min);

            left = self.create_binary_expression(left, &op, right, op_pos);
            self.skip_whitespace();
        }

        left
    }

    fn try_get_operator_precedence(&self, token: &Token) -> Option<(String, i32, bool)> {
        match &token.token_type {
            // All of these are now &&'static str — use directly
            TokenType::ArithmeticOp(op) => {
                OPERATOR_PRECEDENCE.get(op as &str).map(|&(prec, ra)| (op.to_string(), prec, ra))
            }
            TokenType::BitwiseOp(op) => {
                // Bitwise-assign (e.g. "&=") and the null-coalesce "~?" are not binary ops here
                if op.ends_with('=') || *op == "~?" {
                    return None;
                }
                OPERATOR_PRECEDENCE.get(op as &str).map(|&(prec, ra)| (op.to_string(), prec, ra))
            }
            TokenType::ComparisonOp(op) => {
                OPERATOR_PRECEDENCE.get(op as &str).map(|&(prec, ra)| (op.to_string(), prec, ra))
            }
            TokenType::LogicalOp(op) => {
                OPERATOR_PRECEDENCE.get(op as &str).map(|&(prec, ra)| (op.to_string(), prec, ra))
            }
            TokenType::Symbol(sym) => {
                let s = sym.to_string();
                OPERATOR_PRECEDENCE.get(s.as_str()).map(|&(prec, ra)| (s, prec, ra))
            }
            TokenType::Keyword(kw) => {
                // kw: &&'static str — lowercase is already the canonical form for "and"/"or"
                let lower = kw.to_lowercase();
                OPERATOR_PRECEDENCE.get(lower.as_str()).map(|&(prec, ra)| (lower, prec, ra))
            }
            _ => None,
        }
    }

    fn create_binary_expression(
        &self,
        left: Expression,
        op: &str,
        right: Expression,
        position: Position,
    ) -> Expression {
        match op {
            "+" | "-" | "*" | "/" | "%" | "**" | "%%" | "%&" | "&%" => Expression::ArithmeticOp {
                left: Box::new(left),
                operator: op.to_string(),
                right: Box::new(right),
                position,
            },
            ">" | "<" | ">=" | "<=" | "==" | "!=" => Expression::ComparisonOp {
                left: Box::new(left),
                operator: op.to_string(),
                right: Box::new(right),
                position,
            },
            "&&" | "||" | "and" | "or" => Expression::LogicalOp {
                left: Box::new(left),
                operator: op.to_string(),
                right: Box::new(right),
                position,
            },
            "&" | "|" | "^" | "<<" | ">>" => Expression::BitwiseOp {
                left: Box::new(left),
                operator: op.to_string(),
                right: Box::new(right),
                position,
            },
            _ => Expression::ArithmeticOp {
                left: Box::new(left),
                operator: op.to_string(),
                right: Box::new(right),
                position,
            },
        }
    }

    // =============================================================================
    // Unary / primary dispatch
    // =============================================================================

    fn parse_unary_or_primary(&mut self) -> Expression {
        self.skip_whitespace();
        if self.is_at_end() {
            return Expression::Value {
                value: Value::Null { position: Position::UNKNOWN },
                position: Position::UNKNOWN,
            };
        }

        let token = self.current().clone();
        let unary_pos = Position::from_token(&token);

        // Detect unary operator — replace Vec contains check with direct matches!
        let unary_op: Option<String> = match &token.token_type {
            TokenType::Symbol('!') => Some("!".to_string()),
            TokenType::Symbol('-') => Some("-".to_string()),
            TokenType::Symbol('+') => Some("+".to_string()),
            TokenType::ArithmeticOp(op) if *op == "+" || *op == "-" => Some(op.to_string()),
            TokenType::Keyword(kw) if *kw == "not" => Some("not".to_string()),
            TokenType::BitwiseOp(op) if *op == "~?" => Some("~?".to_string()),
            _ => None,
        };

        if let Some(op) = unary_op {
            self.advance();
            self.skip_whitespace();
            let operand = self.parse_primary_base();
            let unary_expr = Expression::UnaryOp {
                operator: op,
                operand: Box::new(operand),
                position: unary_pos,
            };
            return self.apply_postfix_operations(unary_expr);
        }

        self.parse_primary_with_postfix()
    }

    // =============================================================================
    // Postfix: dot-access, index-access, building QualifiedIdentifier chains
    // =============================================================================

    fn apply_postfix_operations(&mut self, mut expr: Expression) -> Expression {
        // Collect initial identifier part for chain detection
        let mut parts: Vec<String> = match &expr {
            Expression::Identifier { name, .. } => vec![name.clone()],
            _ => Vec::new(),
        };

        loop {
            self.skip_whitespace();

            match self.current().token_type {
                TokenType::Symbol('.') => {
                    let dot_pos = Position::from_token(self.current());
                    self.advance();
                    self.skip_whitespace();

                    let member_opt: Option<String> = match &self.current().token_type {
                        TokenType::Identifier(id) => {
                            let name = id.clone();
                            self.advance();
                            Some(name)
                        }
                        TokenType::Keyword(kw)
                        if Keywords::can_be_identifier_in_context(kw, "QUICKFUNCS") =>
                            {
                                let name = kw.to_string(); // &&'static str -> String
                                self.advance();
                                Some(name)
                            }
                        _ => {
                            let cur = self.current().clone();
                            self.error_manager.add_parse_error(
                                ParseErrorType::UnexpectedToken,
                                "Expected identifier after '.'".to_string(),
                                cur.line,
                                cur.column,
                                None,
                                self.get_source_line(&cur),
                            );
                            None
                        }
                    };

                    if let Some(member) = member_opt {
                        if !parts.is_empty() {
                            parts.push(member);
                        } else {
                            expr = Expression::PropertyAccess {
                                object: Box::new(expr),
                                property: member,
                                position: dot_pos,
                            };
                        }
                    } else {
                        break;
                    }
                }

                TokenType::Symbol('[') => {
                    let bracket_pos = Position::from_token(self.current());
                    self.advance();
                    self.skip_whitespace();
                    let index_expr = self.parse_expression(0);
                    self.skip_whitespace();
                    if !self.expect_symbol(']') { break; }

                    // Flush any accumulated parts first
                    if parts.len() >= 2 {
                        let pos = expr.position();
                        expr = Expression::QualifiedIdentifier {
                            parts: std::mem::take(&mut parts),
                            arguments: None,
                            position: pos,
                        };
                    } else {
                        parts.clear();
                    }

                    expr = Expression::IndexAccess {
                        object: Box::new(expr),
                        index: Box::new(index_expr),
                        position: bracket_pos,
                    };
                }

                TokenType::Symbol('(') if !parts.is_empty() => {
                    // A call on a chain: Math.round(x) → QualifiedIdentifier with args
                    let pos = expr.position();
                    let args = self.parse_function_arguments();
                    return Expression::QualifiedIdentifier {
                        parts,
                        arguments: Some(args),
                        position: pos,
                    };
                }

                _ => break,
            }
        }

        // Flush remaining chain
        if parts.len() >= 2 {
            let pos = expr.position();
            self.skip_whitespace();
            if self.check_symbol('(') {
                let args = self.parse_function_arguments();
                return Expression::QualifiedIdentifier {
                    parts,
                    arguments: Some(args),
                    position: pos,
                };
            }
            return Expression::QualifiedIdentifier {
                parts,
                arguments: None,
                position: pos,
            };
        }

        expr
    }

    fn parse_primary_with_postfix(&mut self) -> Expression {
        let expr = self.parse_primary_base();
        self.apply_postfix_operations(expr)
    }

    // =============================================================================
    // Primary expressions
    // =============================================================================

    fn parse_primary_base(&mut self) -> Expression {
        self.skip_whitespace();
        if self.is_at_end() {
            return Expression::Value {
                value: Value::Null { position: Position::UNKNOWN },
                position: Position::UNKNOWN,
            };
        }

        let token = self.current().clone();
        let tok_pos = Position::from_token(&token);

        match &token.token_type {
            TokenType::Integer(i) => {
                let val = *i;
                self.advance();
                Expression::Value {
                    value: Value::Integer { value: val, position: tok_pos },
                    position: tok_pos,
                }
            }
            TokenType::Float(f) => {
                let val = *f;
                self.advance();
                Expression::Value {
                    value: Value::Float { value: val, position: tok_pos },
                    position: tok_pos,
                }
            }
            TokenType::Double(d) => {
                let val = *d;
                self.advance();
                Expression::Value {
                    value: Value::Double { value: val, position: tok_pos },
                    position: tok_pos,
                }
            }
            TokenType::String(s) => {
                let val = s.clone();
                self.advance();
                Expression::Value {
                    value: Value::String { value: val, position: tok_pos },
                    position: tok_pos,
                }
            }
            TokenType::InterpolatedString(template) => {
                let raw = template.clone();
                self.advance();
                let (tpl, exprs) = self.parse_interpolated_string_content(&raw, tok_pos);
                Expression::Value {
                    value: Value::InterpolatedString {
                        template: tpl,
                        expressions: exprs,
                        position: tok_pos,
                    },
                    position: tok_pos,
                }
            }
            TokenType::Bool(b) => {
                let val = *b;
                self.advance();
                Expression::Value {
                    value: Value::Boolean { value: val, position: tok_pos },
                    position: tok_pos,
                }
            }
            // null keyword — Keyword is &&'static str, compare directly
            TokenType::Keyword(kw) if *kw == "null" => {
                self.advance();
                Expression::Value {
                    value: Value::Null { position: tok_pos },
                    position: tok_pos,
                }
            }
            TokenType::Identifier(id) => {
                let name = id.clone();
                self.advance();
                self.skip_whitespace();
                if self.check_symbol('(') {
                    let args = self.parse_function_arguments();
                    return Expression::QuickFuncCall {
                        name,
                        arguments: args,
                        position: tok_pos,
                    };
                }
                Expression::Identifier { name, position: tok_pos }
            }
            TokenType::Symbol('(') => {
                let saved = self.position;
                if self.is_lambda_expression() {
                    return self.parse_lambda_expression();
                }
                self.position = saved;
                self.advance();
                self.skip_whitespace();
                let inner = self.parse_expression(0);
                self.skip_whitespace();
                if !self.expect_symbol(')') {
                    return Expression::Value {
                        value: Value::Null { position: tok_pos },
                        position: tok_pos,
                    };
                }
                Expression::Parenthesized {
                    expression: Box::new(inner),
                    position: tok_pos,
                }
            }
            TokenType::Symbol('[') => {
                let arr = self.parse_array_literal();
                Expression::Value { value: arr, position: tok_pos }
            }
            TokenType::Symbol('{') => {
                let obj = self.parse_object_literal();
                Expression::Value { value: obj, position: tok_pos }
            }
            TokenType::TupleConstructor(_) => self.parse_tuple_constructor(),
            TokenType::BlobConstructor(_)  => self.parse_blob_constructor(),
            TokenType::RegexConstructor(_) => self.parse_regex_constructor(),
            _ => {
                if self.debug_config.is_verbose {
                    self.error_manager.log_debug(&format!(
                        "QuickFunctions: unexpected primary token {}",
                        token.get_token_value()
                    ));
                }
                self.advance();
                Expression::Value {
                    value: Value::Null { position: tok_pos },
                    position: tok_pos,
                }
            }
        }
    }

    // =============================================================================
    // Value / literal parsing
    // =============================================================================

    fn parse_value(&mut self) -> Value {
        self.skip_whitespace();
        if self.is_at_end() {
            return Value::Null { position: Position::UNKNOWN };
        }

        let token = self.current().clone();
        let val_pos = Position::from_token(&token);

        match &token.token_type {
            TokenType::Integer(i)  => { let v = *i; self.advance(); Value::Integer { value: v, position: val_pos } }
            TokenType::Float(f)    => { let v = *f; self.advance(); Value::Float   { value: v, position: val_pos } }
            TokenType::Double(d)   => { let v = *d; self.advance(); Value::Double  { value: v, position: val_pos } }
            TokenType::String(s)   => { let v = s.clone(); self.advance(); Value::String  { value: v, position: val_pos } }
            TokenType::Bool(b)     => { let v = *b; self.advance(); Value::Boolean { value: v, position: val_pos } }
            // Keyword is &&'static str — compare directly with *kw
            TokenType::Keyword(kw) if *kw == "null" => {
                self.advance();
                Value::Null { position: val_pos }
            }
            TokenType::Identifier(id) => {
                let v = id.clone();
                self.advance();
                Value::Identifier { value: v, position: val_pos }
            }
            TokenType::Symbol('[') => self.parse_array_literal(),
            TokenType::Symbol('{') => self.parse_object_literal(),
            TokenType::HexColor(hc) => {
                let v = hc.clone(); self.advance();
                Value::HexColor { value: v, position: val_pos }
            }
            TokenType::Date(d) => {
                let v = d.clone(); self.advance();
                Value::Date { value: v, position: val_pos }
            }
            TokenType::Timestamp(t) => {
                let v = t.clone(); self.advance();
                Value::Timestamp { value: v, position: val_pos }
            }
            TokenType::InterpolatedString(template) => {
                let raw = template.clone();
                self.advance();
                let (tpl, exprs) = self.parse_interpolated_string_content(&raw, val_pos);
                Value::InterpolatedString { template: tpl, expressions: exprs, position: val_pos }
            }
            _ => {
                let v = token.get_token_value();
                self.advance();
                Value::Identifier { value: v, position: val_pos }
            }
        }
    }

    fn parse_array_literal(&mut self) -> Value {
        let arr_pos = Position::from_token(self.current());

        if !self.expect_symbol('[') {
            return Value::Array { values: Vec::new(), position: arr_pos };
        }

        let mut items = Vec::with_capacity(usize::max(4, self.tokens.len() / 40));

        while !self.is_at_end() && !self.check_symbol(']') {
            self.skip_whitespace();
            if self.check_symbol(']') { break; }

            let expr = self.parse_expression(0);
            let item = match expr {
                Expression::Value { value, .. } => value,
                other => Value::Expression {
                    expr: Box::new(other),
                    position: Position::from_token(self.current()),
                },
            };
            items.push(item);
            self.skip_whitespace();

            if self.check_symbol(',') {
                self.advance();
            } else {
                break;
            }
        }

        if self.check_symbol(']') { self.advance(); }

        Value::Array { values: items, position: arr_pos }
    }

    fn parse_object_literal(&mut self) -> Value {
    let obj_pos = Position::from_token(self.current());

    if !self.expect_symbol('{') {
        return Value::Object { properties: Vec::new(), position: obj_pos };
    }

    let mut properties = Vec::with_capacity(estimate_properties_count(self.tokens.len()));

    while !self.is_at_end() && !self.check_symbol('}') {
        self.skip_whitespace();
        if self.check_symbol('}') { break; }

        let prop_pos = Position::from_token(self.current());

        // Property name: identifier, contextual keyword, or quoted string
        let prop_name_opt: Option<String> = match &self.current().token_type {
            TokenType::Identifier(id) => {
                let name = id.clone(); self.advance(); Some(name)
            }
            TokenType::Keyword(kw)
            if Keywords::can_be_identifier_in_context(kw, "QUICKFUNCS") =>
                {
                    let name = kw.to_string(); self.advance(); Some(name)
                }
            TokenType::String(s) => {
                let name = s.clone(); self.advance(); Some(name)
            }
            _ => {
                let cur = self.current().clone();
                self.error_manager.add_parse_error(
                    ParseErrorType::UnexpectedToken,
                    format!("Expected property name, found {}", cur.get_token_value()),
                    cur.line, cur.column, None,
                    self.get_source_line(&cur),
                );
                self.advance();
                None
            }
        };

        let prop_name = match prop_name_opt {
            Some(n) => n,
            None => continue,
        };

        self.skip_whitespace();

        if !self.check_symbol(':') && !self.check_symbol('=') {
            let cur = self.current().clone();
            self.error_manager.add_parse_error(
                ParseErrorType::MissingToken,
                format!("Expected ':' or '=' after property '{}'", prop_name),
                cur.line, cur.column, None,
                self.get_source_line(&cur),
            );
            // Recover: skip to next comma, newline separator, or closing brace
            while !self.is_at_end() && !self.check_symbol(',') && !self.check_symbol('}') {
                self.advance();
            }
            continue;
        }

        self.advance(); // consume ':' or '='
        self.skip_whitespace();

        let val_expr = self.parse_expression(0);
        let prop_value = self.convert_expression_to_value(val_expr);

        properties.push(ObjectProperty::new(prop_name, prop_value, prop_pos));
        self.skip_whitespace();

        // Comma is optional — newline-separated properties are valid DixScript.
        // If there is a comma, consume it and continue.
        // If next token is '}', the outer while condition handles exit.
        // Otherwise just loop — skip_whitespace already consumed the newline.
        if self.check_symbol(',') {
            self.advance();
            self.skip_whitespace();
            // Trailing comma before closing brace is fine
            if self.check_symbol('}') { break; }
        }
        // No else-error here — absence of comma is not an error in DixScript
    }

    if self.check_symbol('}') { self.advance(); }

    Value::Object { properties, position: obj_pos }
        }
    fn parse_tuple_constructor(&mut self) -> Expression {
        let pos = Position::from_token(self.current());
        self.advance();
        self.skip_whitespace();

        if !self.expect_symbol('(') {
            return Expression::Value { value: Value::Null { position: pos }, position: pos };
        }

        let mut args = Vec::new();

        while !self.is_at_end() && !self.check_symbol(')') {
            self.skip_whitespace();
            if self.check_symbol(')') { break; }
            let expr = self.parse_expression(0);
            args.push(Value::Expression {
                expr: Box::new(expr),
                position: Position::from_token(self.current()),
            });
            self.skip_whitespace();
            if self.check_symbol(',') { self.advance(); } else { break; }
        }

        if !self.expect_symbol(')') {
            return Expression::Value { value: Value::Null { position: pos }, position: pos };
        }

        Expression::Value {
            value: Value::PrefixedConstructor { prefix: "t".to_string(), arguments: args, position: pos },
            position: pos,
        }
    }

    fn parse_blob_constructor(&mut self) -> Expression {
        let pos = Position::from_token(self.current());
        self.advance();
        self.skip_whitespace();

        if !self.expect_symbol('(') {
            return Expression::Value { value: Value::Null { position: pos }, position: pos };
        }

        let _data = self.parse_expression(0);
        self.skip_whitespace();

        if !self.expect_symbol(')') {
            return Expression::Value { value: Value::Null { position: pos }, position: pos };
        }

        Expression::Value {
            value: Value::PrefixedConstructor {
                prefix: "b".to_string(),
                arguments: vec![Value::String { value: "blob_data".to_string(), position: pos }],
                position: pos,
            },
            position: pos,
        }
    }

    fn parse_regex_constructor(&mut self) -> Expression {
        let pos = Position::from_token(self.current());
        self.advance();
        self.skip_whitespace();

        if !self.expect_symbol('(') {
            return Expression::Value { value: Value::Null { position: pos }, position: pos };
        }

        let _pattern = self.parse_expression(0);
        self.skip_whitespace();

        if !self.expect_symbol(')') {
            return Expression::Value { value: Value::Null { position: pos }, position: pos };
        }

        Expression::Value {
            value: Value::PrefixedConstructor {
                prefix: "r".to_string(),
                arguments: vec![Value::String { value: "regex_pattern".to_string(), position: pos }],
                position: pos,
            },
            position: pos,
        }
    }

    // =============================================================================
    // Interpolated string
    // =============================================================================

    fn parse_interpolated_string_content(
        &self,
        raw: &str,
        position: Position,
    ) -> (String, Vec<Expression>) {
        let mut expressions = Vec::new();
        let mut template = String::new();
        let mut idx = 0;
        let mut last_end = 0;

        for cap in INTERP_PLACEHOLDER_RE.captures_iter(raw) {
            let m0 = cap.get(0).unwrap();
            let expr_text = cap.get(1).unwrap().as_str();

            template.push_str(&raw[last_end..m0.start()]);
            expressions.push(self.parse_interpolated_expression(expr_text, position));
            template.push_str(&format!("{{{}}}", idx));
            idx += 1;
            last_end = m0.end();
        }

        template.push_str(&raw[last_end..]);
        (template, expressions)
    }

    fn parse_interpolated_expression(&self, text: &str, position: Position) -> Expression {
        let trimmed = text.trim();

        if let Ok(v) = trimmed.parse::<i32>() {
            return Expression::Value {
                value: Value::Integer { value: v, position },
                position,
            };
        }

        if trimmed.ends_with('f') || trimmed.ends_with('F') {
            if let Ok(v) = trimmed[..trimmed.len() - 1].parse::<f32>() {
                return Expression::Value {
                    value: Value::Float { value: v, position },
                    position,
                };
            }
        }

        // Only attempt f64 parse on purely numeric strings to avoid treating "obj.prop" as double
        if trimmed.contains('.') {
            let is_numeric = trimmed.chars().all(|c| c.is_ascii_digit() || matches!(c, '.' | 'e' | 'E' | '+' | '-'));
            if is_numeric {
                if let Ok(v) = trimmed.parse::<f64>() {
                    return Expression::Value {
                        value: Value::Double { value: v, position },
                        position,
                    };
                }
            }
        }

        if trimmed.eq_ignore_ascii_case("true") {
            return Expression::Value { value: Value::Boolean { value: true, position }, position };
        }
        if trimmed.eq_ignore_ascii_case("false") {
            return Expression::Value { value: Value::Boolean { value: false, position }, position };
        }

        if (trimmed.starts_with('"') && trimmed.ends_with('"'))
            || (trimmed.starts_with('\'') && trimmed.ends_with('\''))
        {
            return Expression::Value {
                value: Value::String { value: trimmed[1..trimmed.len() - 1].to_string(), position },
                position,
            };
        }

        if let Some(cap) = INTERP_METHOD_CALL_RE.captures(trimmed) {
            return Expression::QualifiedIdentifier {
                parts: vec![cap[1].to_string(), cap[2].to_string()],
                arguments: Some(Vec::new()),
                position,
            };
        }

        if let Some(cap) = INTERP_PROPERTY_RE.captures(trimmed) {
            return Expression::QualifiedIdentifier {
                parts: vec![cap[1].to_string(), cap[2].to_string()],
                arguments: None,
                position,
            };
        }

        Expression::Identifier { name: trimmed.to_string(), position }
    }

    // =============================================================================
    // Function arguments
    // =============================================================================

    fn parse_function_arguments(&mut self) -> Vec<Expression> {
        let mut args = Vec::with_capacity(usize::max(2, self.tokens.len() / 50));

        if !self.expect_symbol('(') {
            return args;
        }
        self.skip_whitespace();

        if self.check_symbol(')') {
            self.advance();
            return args;
        }

        loop {
            self.skip_whitespace();
            args.push(self.parse_expression(0));
            self.skip_whitespace();

            if self.check_symbol(',') {
                self.advance();
                self.skip_whitespace();
                if self.check_symbol(')') {
                    let cur = self.current().clone();
                    self.error_manager.add_parse_error(
                        ParseErrorType::UnexpectedToken,
                        "Trailing comma in function arguments".to_string(),
                        cur.line, cur.column, None,
                        self.get_source_line(&cur),
                    );
                    break;
                }
            } else {
                break;
            }
        }

        if !self.expect_symbol(')') {
            let cur = self.current().clone();
            self.error_manager.add_parse_error(
                ParseErrorType::MissingToken,
                "Expected ')' after function arguments".to_string(),
                cur.line, cur.column, None,
                self.get_source_line(&cur),
            );
        }

        args
    }

    fn convert_expression_to_value(&self, expr: Expression) -> Value {
        let pos = expr.position();
        match expr {
            Expression::Value { value, .. } => value,
            Expression::Identifier { name, .. } => Value::Identifier { value: name, position: pos },
            Expression::QualifiedIdentifier { parts, .. } => {
                Value::Identifier { value: parts.join("."), position: pos }
            }
            other => Value::Expression { expr: Box::new(other), position: pos },
        }
    }

    // =============================================================================
    // Lambda expressions
    // =============================================================================

    fn is_lambda_expression(&self) -> bool {
        if !self.check_symbol('(') {
            return false;
        }

        let mut look = self.position;
        let mut depth: i32 = 0;

        while look < self.tokens.len() {
            match self.tokens[look].token_type {
                TokenType::Symbol('(') => depth += 1,
                TokenType::Symbol(')') => {
                    depth -= 1;
                    if depth == 0 {
                        look += 1;
                        // Skip whitespace-like tokens
                        while look < self.tokens.len() {
                            let v = self.tokens[look].get_token_value();
                            if v.trim().is_empty() { look += 1; } else { break; }
                        }
                        if look < self.tokens.len() {
                            return matches!(
                                self.tokens[look].token_type,
                                TokenType::Arrow | TokenType::Symbol('=')
                            );
                        }
                        return false;
                    }
                }
                _ => {}
            }
            look += 1;
        }
        false
    }

    fn parse_lambda_expression(&mut self) -> Expression {
        let lambda_pos = Position::from_token(self.current());
        let parameters = self.parse_lambda_parameters();
        self.skip_whitespace();

        // Consume `=>` (Arrow token) or `=` `>` two-symbol fallback
        if matches!(self.current().token_type, TokenType::Arrow) {
            self.advance();
        } else if self.check_symbol('=') {
            self.advance();
            if !self.expect_symbol('>') {
                let cur = self.current().clone();
                self.error_manager.add_parse_error(
                    ParseErrorType::MissingToken,
                    "Expected '=>' after lambda parameters".to_string(),
                    cur.line, cur.column, None,
                    self.get_source_line(&cur),
                );
                return Expression::Value { value: Value::Null { position: lambda_pos }, position: lambda_pos };
            }
        } else {
            let cur = self.current().clone();
            self.error_manager.add_parse_error(
                ParseErrorType::MissingToken,
                "Expected '=>' after lambda parameters".to_string(),
                cur.line, cur.column, None,
                self.get_source_line(&cur),
            );
            return Expression::Value { value: Value::Null { position: lambda_pos }, position: lambda_pos };
        }

        self.skip_whitespace();
        let body = if self.check_symbol('{') {
            self.parse_lambda_block_body()
        } else {
            self.parse_expression(0)
        };

        let param_count = parameters.len();
        let result = Expression::Value {
            value: Value::Lambda {
                parameters,
                body: Box::new(body),
                position: lambda_pos,
            },
            position: lambda_pos,
        };

        if self.debug_config.is_verbose {
            self.error_manager.log_debug(&format!(
                "QuickFunctions: lambda with {} params",
                param_count
            ));
        }

        result
    }

    fn parse_lambda_parameters(&mut self) -> Vec<String> {
        let mut params = Vec::new();

        if !self.expect_symbol('(') {
            return params;
        }
        self.skip_whitespace();

        if self.check_symbol(')') {
            self.advance();
            return params;
        }

        loop {
            self.skip_whitespace();

            if let TokenType::Identifier(id) = &self.current().token_type {
                let name = id.clone();
                self.advance();
                self.skip_whitespace();

                // Skip optional type annotation
                if self.check_symbol('<') {
                    self.advance();
                    self.skip_whitespace();
                    if matches!(
                        self.current().token_type,
                        TokenType::Keyword(_) | TokenType::Identifier(_)
                    ) {
                        self.advance();
                    }
                    self.skip_whitespace();
                    if !self.expect_symbol('>') { break; }
                    self.skip_whitespace();
                }

                params.push(name);

                if self.check_symbol(',') {
                    self.advance();
                } else {
                    break;
                }
            } else {
                let cur = self.current().clone();
                self.error_manager.add_parse_error(
                    ParseErrorType::UnexpectedToken,
                    "Expected parameter name in lambda".to_string(),
                    cur.line, cur.column, None,
                    self.get_source_line(&cur),
                );
                break;
            }
        }

        if !self.expect_symbol(')') {
            let cur = self.current().clone();
            self.error_manager.add_parse_error(
                ParseErrorType::MissingToken,
                "Expected ')' after lambda parameters".to_string(),
                cur.line, cur.column, None,
                self.get_source_line(&cur),
            );
        }

        params
    }

    fn parse_lambda_block_body(&mut self) -> Expression {
        let pos = Position::from_token(self.current());

        if !self.expect_symbol('{') {
            return Expression::Value { value: Value::Null { position: pos }, position: pos };
        }

        let mut stmt_count = 0usize;

        while !self.is_at_end() && !self.check_symbol('}') {
            self.skip_whitespace();
            if self.check_symbol('}') { break; }
            if self.parse_statement().is_some() {
                stmt_count += 1;
            }
            self.skip_whitespace();
        }

        if !self.expect_symbol('}') {
            let cur = self.current().clone();
            self.error_manager.add_parse_error(
                ParseErrorType::MissingToken,
                "Expected '}' to close lambda block".to_string(),
                cur.line, cur.column, None,
                self.get_source_line(&cur),
            );
        }

        // Represent the block as an opaque identifier value — the AST enhancer owns resolution.
        Expression::Value {
            value: Value::Identifier {
                value: format!("<lambda_block:{}_stmts>", stmt_count),
                position: pos,
            },
            position: pos,
        }
    }

    // =============================================================================
    // Helpers
    // =============================================================================

    /// Convert a type-keyword string to the corresponding DataType variant.
    fn str_to_data_type(s: &str) -> Option<DataType> {
        match s {
            "int"       => Some(DataType::Int),
            "float"     => Some(DataType::Float),
            "double"    => Some(DataType::Double),
            "string"    => Some(DataType::String),
            "bool"      => Some(DataType::Bool),
            "array"     => Some(DataType::Array),
            "object"    => Some(DataType::Object),
            "tuple"     => Some(DataType::Tuple),
            "hex"       => Some(DataType::Hex),
            "blob"      => Some(DataType::Blob),
            "regex"     => Some(DataType::Regex),
            "date"      => Some(DataType::Date),
            "timestamp" => Some(DataType::Timestamp),
            "enum"      => Some(DataType::Enum),
            "any"       => Some(DataType::Any),
            _           => None,
        }
    }

    fn parse_type_annotation(&mut self) -> Option<DataType> {
        if !self.check_symbol('<') {
            return None;
        }
        self.advance(); // consume '<'
        self.skip_whitespace();

        let type_token = self.current().clone();

        let type_str = match &type_token.token_type {
            TokenType::Keyword(kw)    => Some(kw.to_lowercase()),
            TokenType::Identifier(id) => Some(id.to_lowercase()),
            _ => None,
        };

        let dt = if let Some(ref s) = type_str {
            let result = Self::str_to_data_type(s);
            if result.is_none() {
                self.error_manager.add_parse_error(
                    ParseErrorType::InvalidType,
                    format!("Invalid type annotation '{}'", s),
                    type_token.line, type_token.column, None,
                    self.get_source_line(&type_token),
                );
            }
            result
        } else {
            self.error_manager.add_parse_error(
                ParseErrorType::InvalidType,
                format!("Expected type name inside '<>', found {}", type_token.get_token_value()),
                type_token.line, type_token.column, None,
                self.get_source_line(&type_token),
            );
            None
        };

        if type_str.is_some() {
            self.advance(); // consume the type token
        } else {
            self.advance(); // skip invalid
        }

        self.skip_whitespace();

        // Recover from a missing '>' by scanning forward
        if !self.check_symbol('>') {
            let cur = self.current().clone();
            self.error_manager.add_parse_error(
                ParseErrorType::MissingToken,
                "Expected '>' to close type annotation".to_string(),
                cur.line, cur.column, None,
                self.get_source_line(&cur),
            );
            let mut depth: i32 = 1;
            while !self.is_at_end() && depth > 0 {
                match self.current().token_type {
                    TokenType::Symbol('<') => depth += 1,
                    TokenType::Symbol('>') => { depth -= 1; if depth == 0 { self.advance(); break; } }
                    _ => {}
                }
                self.advance();
            }
        } else {
            self.advance(); // consume '>'
        }

        dt
    }

    fn parse_dotted_path(&mut self) -> Option<String> {
        let mut path = match &self.current().token_type {
            TokenType::Identifier(id) => { let p = id.clone(); self.advance(); p }
            TokenType::Keyword(kw)    => { let p = kw.to_string(); self.advance(); p }
            _ => return None,
        };

        while self.check_symbol('.') {
            self.advance();
            self.skip_whitespace();

            match &self.current().token_type {
                TokenType::Identifier(id) => {
                    path.push('.');
                    path.push_str(id);
                    self.advance();
                }
                TokenType::Keyword(kw) if Keywords::can_be_identifier_in_context(kw, "QUICKFUNCS") => {
                    let segment = kw.to_string(); // &&'static str -> String
                    path.push('.');
                    path.push_str(&segment);
                    self.advance();
                }
                _ => {
                    let cur = self.current().clone();
                    self.error_manager.add_parse_error(
                        ParseErrorType::UnexpectedToken,
                        "Expected identifier after '.' in path".to_string(),
                        cur.line, cur.column, None,
                        self.get_source_line(&cur),
                    );
                    break;
                }
            }
        }

        Some(path)
    }

    /// Consume a `->`  switch-case arrow (distinct from `=>` scope arrow).
    fn match_arrow(&mut self) -> bool {
        if let TokenType::MultiCharSymbol(ms) = &self.current().token_type {
            if *ms == "->" {
                self.advance();
                return true;
            }
        }
        if matches!(self.current().token_type, TokenType::SwitchCase) {
            self.advance();
            return true;
        }
        // Two-symbol fallback: '-' '>'
        if matches!(self.current().token_type, TokenType::Symbol('-')) {
            if self.position + 1 < self.tokens.len() {
                if matches!(self.tokens[self.position + 1].token_type, TokenType::Symbol('>')) {
                    self.advance();
                    self.advance();
                    return true;
                }
            }
        }
        false
    }

    /// Check for `=>` scope/lambda arrow without consuming.
    fn check_arrow(&self) -> bool {
        if let TokenType::MultiCharSymbol(ms) = &self.current().token_type {
            // ms: &&'static str — compare directly
            return *ms == "=>";
        }
        matches!(self.current().token_type, TokenType::Arrow)
    }

    // =============================================================================
    // Token navigation
    // =============================================================================

    #[inline]
    fn current(&self) -> &Token {
        static EOF_TOKEN: Token = Token {
            token_type: TokenType::EndOfFile,
            line: 1,
            column: 1,
            section: SectionId::None,
        };
        self.tokens.get(self.position).unwrap_or(&EOF_TOKEN)
    }

    #[inline]
    fn is_at_end(&self) -> bool {
        self.position >= self.tokens.len()
            || matches!(self.current().token_type, TokenType::EndOfFile)
    }

    #[inline]
    fn advance(&mut self) {
        if self.position < self.tokens.len() {
            self.position += 1;
        }
    }

    #[inline]
    fn check_symbol(&self, symbol: char) -> bool {
        matches!(&self.current().token_type, TokenType::Symbol(s) if *s == symbol)
    }

    #[inline]
    fn expect_symbol(&mut self, symbol: char) -> bool {
        if self.check_symbol(symbol) {
            self.advance();
            true
        } else {
            let cur = self.current().clone();
            self.error_manager.add_parse_error(
                ParseErrorType::MissingToken,
                format!("Expected '{}'", symbol),
                cur.line, cur.column, None,
                self.get_source_line(&cur),
            );
            false
        }
    }

    fn skip_whitespace(&mut self) {
        while !self.is_at_end() {
            match &self.current().token_type {
                // Never skip string tokens — they are content
                TokenType::String(_)
                | TokenType::StringSingle(_)
                | TokenType::InterpolatedString(_) => break,
                // Strip comments
                TokenType::Comment(_) => { self.advance(); continue; }
                _ => {
                    let v = self.current().get_token_value();
                    if v.trim().is_empty() || v == "\n" || v == "\r" || v == "\t" {
                        self.advance();
                    } else {
                        break;
                    }
                }
            }
        }
    }

    fn get_source_line(&self, token: &Token) -> Option<String> {
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
}
