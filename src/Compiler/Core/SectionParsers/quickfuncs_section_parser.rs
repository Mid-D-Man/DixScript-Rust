// src/Compiler/Core/SectionParsers/quickfuncs_section_parser.rs

use crate::Compiler::AST::{
    QuickFuncsSection, QuickFunction, QuickFuncParam, QuickFuncStatement, SwitchCase,
    Position, DataType, Expression, Value, ObjectProperty, DeclarationType,
};
use crate::Compiler::Core::{OperationalSettings, ErrorHandlingStrategy, DebugMode};
use crate::ErrorManager::{ErrorManager, ParseErrorType};
use crate::Compiler::Core::Tokenizer::{Token, TokenType};
use crate::Utilities::{Keywords, estimate_statements_count, estimate_properties_count};
use std::collections::HashMap;

/// QuickFunctions Section Parser v1.0.0 - Simplified identifier handling
/// All dotted identifiers become QualifiedIdentifier - analyzer resolves them
/// Syntax: ~name<returnType> => scope (params) { body }
pub struct QuickFuncsSectionParser<'a> {
    tokens: &'a [Token],
    operational_settings: &'a OperationalSettings,
    error_manager: ErrorManager,

    // Parse state
    position: usize,
    last_position: usize,
    stuck_count: usize,
    iteration_count: usize,
}

// ==================== CONSTANTS AND CONFIGURATION ====================

const MAX_ITERATIONS_PER_TOKEN: usize = 3;
const ABSOLUTE_MAX_ITERATIONS: usize = 1_000_000;
const MAX_STUCK_COUNT: usize = 3;

// ==================== OPERATOR PRECEDENCE TABLE ====================

lazy_static::lazy_static! {
    static ref OPERATOR_PRECEDENCE: HashMap<&'static str, (i32, bool)> = {
        let mut m = HashMap::new();
        m.insert("**", (13, true));
        m.insert("*", (12, false));
        m.insert("/", (12, false));
        m.insert("%", (12, false));
        m.insert("%%", (12, false));
        m.insert("%&", (12, false));
        m.insert("&%", (12, false));
        m.insert("+", (11, false));
        m.insert("-", (11, false));
        m.insert("<<", (10, false));
        m.insert(">>", (10, false));
        m.insert(">", (9, false));
        m.insert("<", (9, false));
        m.insert(">=", (9, false));
        m.insert("<=", (9, false));
        m.insert("==", (8, false));
        m.insert("!=", (8, false));
        m.insert("&", (7, false));
        m.insert("^", (6, false));
        m.insert("|", (5, false));
        m.insert("&&", (4, false));
        m.insert("and", (4, false));
        m.insert("||", (3, false));
        m.insert("or", (3, false));
        m
    };

    static ref VALID_UNARY_OPERATORS: Vec<&'static str> = {
        vec!["!", "not", "-", "+", "~?"]
    };
}

impl<'a> QuickFuncsSectionParser<'a> {
    // ==================== CONSTRUCTOR ====================

    /// Create a new QuickFunctions section parser
    pub fn new(
        tokens: &'a [Token],
        operational_settings: &'a OperationalSettings,
    ) -> Self {
        let error_manager = ErrorManager::get_shared_instance();

        error_manager.log_debug(&format!(
            "Initializing QuickFunctions parser v1.0.0 with {} tokens",
            tokens.len()
        ));

        QuickFuncsSectionParser {
            tokens,
            operational_settings,
            error_manager,
            position: 0,
            last_position: usize::MAX,
            stuck_count: 0,
            iteration_count: 0,
        }
    }

    // ==================== MAIN PARSE METHOD ====================

    /// Parse the QUICKFUNCS section
    pub fn parse_section(&mut self) -> Option<QuickFuncsSection> {
        self.log_debug("Starting QUICKFUNCS section parse");

        let section_start_token = self.current().clone();
        let section_start_pos = Position::from_token(&section_start_token);

        let estimated_functions = usize::max(2, self.tokens.len() / 50);
        let mut functions = Vec::with_capacity(estimated_functions);

        // Expect opening parenthesis
        if self.check_symbol('(') {
            self.advance();
            self.log_debug("Consumed opening parenthesis");
        }

        let max_iterations = (self.tokens.len() * MAX_ITERATIONS_PER_TOKEN)
            .min(ABSOLUTE_MAX_ITERATIONS);

        while !self.is_at_end()
            && !self.check_symbol(')')
            && self.iteration_count < max_iterations
        {
            self.skip_whitespace();
            if self.is_at_end() || self.check_symbol(')') {
                break;
            }

            self.iteration_count += 1;

            let position_before = self.position;

            if self.check_symbol('~') {
                match self.parse_function() {
                    Some(func) => {
                        self.log_debug(&format!("Parsed function: {}", func.name));
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

                if let TokenType::Symbol(';' | ',') = current.token_type {
                    self.log_verbose(&format!("Skipping stray symbol '{}'",
                                              current.get_token_value()));
                    self.advance();
                    continue;
                }

                self.error_manager.add_parse_error(
                    ParseErrorType::UnexpectedToken,
                    format!("Expected '~' to start function, found {}",
                            current.get_token_value()),
                    current.line,
                    current.column,
                    None,
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
                self.log_verbose(&format!("Forced advancement from position {}",
                                          self.position));
                self.advance();
            }
        }

        self.error_manager.log_info(&format!(
            "Successfully parsed {} functions",
            functions.len()
        ));

        Some(QuickFuncsSection::new(functions, section_start_pos))
    }

    // ==================== FUNCTION STRUCTURE PARSING ====================

    /// Parse complete function definition
    /// Syntax: ~name<returnType> => scope (params) { body }
    fn parse_function(&mut self) -> Option<QuickFunction> {
        let function_start_token = self.current().clone();
        let function_start_position = Position::from_token(&function_start_token);

        // Expect '~'
        if !self.expect_symbol('~') {
            return None;
        }

        // Parse function name
        let name_token = self.current().clone();
        let function_name = match &name_token.token_type {
            TokenType::Identifier(id) => id.clone(),
            _ => {
                self.error_manager.add_parse_error(
                    ParseErrorType::MissingToken,
                    "Expected function name after '~'".to_string(),
                    name_token.line,
                    name_token.column,
                    None,
                    self.get_source_line(&name_token),
                );
                return None;
            }
        };

        self.advance();
        self.skip_whitespace();

        self.log_debug(&format!("Parsing function: {}", function_name));

        // Parse return type
        let return_type = if self.check_symbol('<') {
            self.parse_return_type()
        } else {
            None
        };

        self.skip_whitespace();

        // Parse scope declaration
        let scope_list = if self.check_arrow() {
            self.parse_scope_declaration()
        } else {
            Some(vec!["global".to_string()])
        };

        self.skip_whitespace();

        // Parse parameters
        let parameters = if self.check_symbol('(') {
            self.parse_parameters()
        } else {
            Vec::new()
        };

        self.skip_whitespace();

        // Parse body
        let body = if self.check_symbol('{') {
            self.parse_statement_block()
        } else {
            self.error_manager.add_parse_error(
                ParseErrorType::MissingToken,
                "Expected '{' to start function body".to_string(),
                self.current().line,
                self.current().column,
                None,
                self.get_source_line(self.current()),
            );
            return None;
        };

        self.error_manager.log_info(&format!(
            "Function {} complete: return={:?}, scope={:?}, params={}, stmts={}",
            function_name,
            return_type,
            scope_list,
            parameters.len(),
            body.len()
        ));

        Some(QuickFunction::new(
            function_name,
            return_type,
            scope_list,
            parameters,
            body,
            function_start_position,
        ))
    }

    fn parse_return_type(&mut self) -> Option<DataType> {
        self.parse_type_annotation()
    }

    /// Parse scope declaration: => global, => data.users, => data.users, data.posts
    fn parse_scope_declaration(&mut self) -> Option<Vec<String>> {
        let mut scopes = Vec::new();

        if !self.check_arrow() {
            return Some(scopes);
        }

        // Consume arrow
        self.advance();

        loop {
            self.skip_whitespace();

            let token = self.current().clone();

            let scope_path = if let TokenType::Keyword(kw) = &token.token_type {
                if kw.as_str() == "global" {
                    self.advance();
                    Some("global".to_string())
                } else if Keywords::can_be_identifier_in_context(kw, "QUICKFUNCS") {
                    self.parse_dotted_path()
                } else {
                    self.error_manager.add_parse_error(
                        ParseErrorType::UnexpectedToken,
                        format!("Cannot use language keyword '{}' in scope path", kw),
                        token.line,
                        token.column,
                        None,
                        self.get_source_line(&token),
                    );
                    None
                }
            } else if let TokenType::Identifier(_) = &token.token_type {
                self.parse_dotted_path()
            } else {
                self.error_manager.add_parse_error(
                    ParseErrorType::UnexpectedToken,
                    format!("Expected scope identifier or 'global' after '=>', found {}",
                            token.get_token_value()),
                    token.line,
                    token.column,
                    None,
                    self.get_source_line(&token),
                );
                None
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

        self.log_verbose(&format!("Parsed {} scope(s): {:?}",
                                  scopes.len(), scopes));

        Some(scopes)
    }

    /// Parse function parameter list
    /// Syntax: (x<int>, y<float> = 42, z = getValue())
    fn parse_parameters(&mut self) -> Vec<QuickFuncParam> {
        let estimated_params = usize::max(2, self.tokens.len() / 100);
        let mut parameters = Vec::with_capacity(estimated_params);

        if !self.expect_symbol('(') {
            return parameters;
        }

        self.skip_whitespace();

        if self.check_symbol(')') {
            self.advance();
            self.log_verbose("Empty parameter list");
            return parameters;
        }

        loop {
            self.skip_whitespace();

            let param_start_token = self.current().clone();
            let param_position = Position::from_token(&param_start_token);

            // Parse parameter name
            let param_name = match &self.current().token_type {
                TokenType::Identifier(id) => {
                    let name = id.clone();
                    self.advance();
                    Some(name)
                }
                TokenType::Keyword(kw)
                if Keywords::can_be_identifier_in_context(kw, "QUICKFUNCS") =>
                    {
                        let name = kw.clone();
                        self.advance();
                        self.log_verbose(&format!("Accepted keyword '{}' as parameter name", name));
                        Some(name)
                    }
                _ => {
                    self.error_manager.add_parse_error(
                        ParseErrorType::MissingToken,
                        "Expected parameter name".to_string(),
                        self.current().line,
                        self.current().column,
                        None,
                        self.get_source_line(self.current()),
                    );
                    None
                }
            };

            if param_name.is_none() {
                break;
            }

            let param_name = param_name.unwrap();
            self.skip_whitespace();

            let mut param_type: Option<DataType> = None;
            let mut default_value: Option<Expression> = None;

            // Parse type annotation INLINE (to handle default values inside <...>)
            if self.check_symbol('<') {
                self.log_verbose(&format!("Parsing type annotation for parameter '{}'", param_name));
                self.advance();  // consume '<'
                self.skip_whitespace();

                let type_token = self.current().clone();

                // Parse the type keyword
                param_type = match &type_token.token_type {
                    TokenType::Keyword(kw) | TokenType::Identifier(kw) => {
                        match kw.to_lowercase().as_str() {
                            "int" => Some(DataType::Int),
                            "float" => Some(DataType::Float),
                            "double" => Some(DataType::Double),
                            "string" => Some(DataType::String),
                            "bool" => Some(DataType::Bool),
                            "array" => Some(DataType::Array),
                            "object" => Some(DataType::Object),
                            "tuple" => Some(DataType::Tuple),
                            "hex" => Some(DataType::Hex),
                            "blob" => Some(DataType::Blob),
                            "regex" => Some(DataType::Regex),
                            "date" => Some(DataType::Date),
                            "timestamp" => Some(DataType::Timestamp),
                            "enum" => Some(DataType::Enum),
                            "any" => Some(DataType::Any),
                            _ => None,
                        }
                    }
                    _ => None,
                };

                if param_type.is_some() {
                    self.log_verbose(&format!("Found type: {:?}", param_type));
                    self.advance();  // consume type keyword
                    self.skip_whitespace();
                } else {
                    self.error_manager.add_parse_error(
                        ParseErrorType::InvalidType,
                        format!("Invalid parameter type: {}", type_token.get_token_value()),
                        type_token.line,
                        type_token.column,
                        None,
                        self.get_source_line(&type_token),
                    );
                    self.advance();  // skip invalid token
                    self.skip_whitespace();
                }

                // CHECK FOR '=' BEFORE THE CLOSING '>' (default value inside type annotation)
                if self.check_symbol('=') {
                    self.log_verbose("Found '=' inside type annotation for default value");
                    self.advance();
                    self.skip_whitespace();
                    default_value = Some(self.parse_expression(0));
                    self.skip_whitespace();
                }

                // Expect closing '>'
                if !self.expect_symbol('>') {
                    self.log_verbose("Missing '>' after type annotation");
                    break;
                }
            }

            self.skip_whitespace();

            // ALSO check for '=' OUTSIDE the type annotation (default value after type)
            if self.check_symbol('=') && default_value.is_none() {
                self.log_verbose(&format!("Found '=' outside type annotation for parameter '{}'", param_name));
                self.advance();
                self.skip_whitespace();
                default_value = Some(self.parse_expression(0));
                self.skip_whitespace();
            }

            parameters.push(QuickFuncParam::new(
                param_name.clone(),
                param_type,
                default_value.clone(),
                param_position,
            ));

            self.log_verbose(&format!(
                "Added parameter: {} <{:?}> = {:?}",
                param_name,
                param_type,
                default_value.as_ref().map(|_| "expression")
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

        self.log_verbose(&format!("Parsed {} parameters total", parameters.len()));
        parameters
    }

    /// Parse statement block
    /// Syntax: { statement1 statement2 ... }
    fn parse_statement_block(&mut self) -> Vec<QuickFuncStatement> {
        let estimated_stmts = estimate_statements_count(self.tokens.len());
        let mut statements = Vec::with_capacity(estimated_stmts);

        if !self.expect_symbol('{') {
            return statements;
        }

        self.log_verbose(&format!("Parsing statement block starting at position {}",
                                  self.position));

        let mut brace_depth = 1;
        let max_iterations = (self.tokens.len() * MAX_ITERATIONS_PER_TOKEN)
            .min(ABSOLUTE_MAX_ITERATIONS);

        while !self.is_at_end() && brace_depth > 0 && self.iteration_count < max_iterations {
            self.skip_whitespace();

            if let TokenType::Symbol('}') = self.current().token_type {
                brace_depth -= 1;
                if brace_depth == 0 {
                    self.log_verbose(&format!("Found closing brace at position {}",
                                              self.position));
                    self.advance();
                    break;
                }
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
                self.log_verbose(&format!("Forced advancement from position {}",
                                          self.position));

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

        if brace_depth > 0 && self.check_symbol('}') {
            self.log_verbose(&format!("Consuming remaining closing brace at position {}",
                                      self.position));
            self.advance();
        }

        self.log_verbose(&format!("Parsed {} statements in block", statements.len()));
        statements
    }

    // ==================== STATEMENT PARSING ====================

    /// Parse statement with support for let/const variable declarations
    /// Syntax: let x = 5, let mut y<int> = 10, const z = 15, x += 5
    fn parse_statement(&mut self) -> Option<QuickFuncStatement> {
        self.skip_whitespace();
        if self.is_at_end() {
            return None;
        }

        let token = self.current().clone();
        let statement_position = Position::from_token(&token);
        let start_position = self.position;

        self.log_verbose(&format!("ParseStatement starting at position {}, token: {}",
                                  start_position, token.get_token_value()));

        // Check for closing brace
        if let TokenType::Symbol('}') = token.token_type {
            self.log_verbose("Found closing brace - not consuming (belongs to parent scope)");
            return None;
        }

        // Return statement
        if let TokenType::Keyword(kw) = &token.token_type {
            if kw.as_str() == "return" {
                self.advance();
                self.skip_whitespace();

                let expr = if !self.check_symbol(';') && !self.check_symbol('}') {
                    self.parse_expression(0)
                } else {
                    Expression::Value {
                        value: Value::Null { position: statement_position },
                        position: statement_position,
                    }
                };

                self.skip_whitespace();
                if self.check_symbol(';') {
                    self.advance();
                }

                return Some(QuickFuncStatement::Return {
                    value: expr,
                    position: statement_position,
                });
            }

            // If statement
            if kw.as_str() == "if" {
                return self.parse_if_statement();
            }

            // Switch statement
            if kw.as_str() == "chk" {
                return self.parse_switch_statement();
            }

            // Log statement
            if kw.as_str() == "log" {
                return self.parse_log_statement(statement_position);
            }

            // Let declaration
            if kw.as_str() == "let" {
                return Some(self.parse_variable_declaration(
                    DeclarationType::Let,
                    statement_position
                ));
            }

            // Const declaration
            if kw.as_str() == "const" {
                return Some(self.parse_variable_declaration(
                    DeclarationType::Const,
                    statement_position
                ));
            }
        }

        // Log statement (identifier variant)
        if let TokenType::Identifier(id) = &token.token_type {
            if id.eq_ignore_ascii_case("log") {
                return self.parse_log_statement(statement_position);
            }
        }

        // Assignment or expression statement
        if let TokenType::Identifier(var_name) = &token.token_type {
            let var_name = var_name.clone();
            let saved_position = self.position;

            self.advance();
            self.skip_whitespace();

            let next_token = self.current();

            // Regular assignment
            if let TokenType::Symbol('=') = next_token.token_type {
                self.advance();
                self.skip_whitespace();
                let expr = self.parse_expression(0);
                self.skip_whitespace();
                if self.check_symbol(';') {
                    self.advance();
                }
                return Some(QuickFuncStatement::Assignment {
                    variable: var_name,
                    value: expr,
                    position: statement_position,
                });
            }

            // Arithmetic assignment
            if let TokenType::ArithmeticAssignOp(op) = &next_token.token_type {
                let operator = op.clone();
                self.advance();
                self.skip_whitespace();
                let expr = self.parse_expression(0);
                self.skip_whitespace();
                if self.check_symbol(';') {
                    self.advance();
                }
                return Some(QuickFuncStatement::ArithmeticAssignment {
                    variable: var_name,
                    operator,
                    value: expr,
                    position: statement_position,
                });
            }

            // Bitwise assignment
            if let TokenType::BitwiseOp(op) = &next_token.token_type {
                if op.ends_with('=') {
                    let operator = op.clone();
                    self.advance();
                    self.skip_whitespace();
                    let expr = self.parse_expression(0);
                    self.skip_whitespace();
                    if self.check_symbol(';') {
                        self.advance();
                    }
                    return Some(QuickFuncStatement::ArithmeticAssignment {
                        variable: var_name,
                        operator,
                        value: expr,
                        position: statement_position,
                    });
                }
            }

            // Not assignment, parse as expression
            self.position = saved_position;
        }

        // Keyword assignment (contextual keywords)
        if let TokenType::Keyword(kw) = &token.token_type {
            if Keywords::can_be_identifier_in_context(kw, "QUICKFUNCS") {
                let var_name = kw.clone();
                let saved_position = self.position;

                self.advance();
                self.skip_whitespace();

                let next_token = self.current();

                if let TokenType::Symbol('=') = next_token.token_type {
                    self.advance();
                    self.skip_whitespace();
                    let expr = self.parse_expression(0);
                    self.skip_whitespace();
                    if self.check_symbol(';') {
                        self.advance();
                    }
                    self.log_verbose(&format!("Accepted keyword '{}' as variable name",
                                              var_name));
                    return Some(QuickFuncStatement::Assignment {
                        variable: var_name,
                        value: expr,
                        position: statement_position,
                    });
                }

                self.position = saved_position;
            }
        }

        // Fallback: expression statement
        let fallback_expr = self.parse_expression(0);
        self.skip_whitespace();
        if self.check_symbol(';') {
            self.advance();
        }
        Some(QuickFuncStatement::ExpressionStatement {
            expression: fallback_expr,
            position: statement_position,
        })
    }

    fn parse_log_statement(&mut self, position: Position) -> Option<QuickFuncStatement> {
        self.advance();
        self.skip_whitespace();

        if !self.check_symbol(':') {
            self.error_manager.add_parse_error(
                ParseErrorType::MissingToken,
                "Expected ':' after 'log' keyword".to_string(),
                self.current().line,
                self.current().column,
                None,
                self.get_source_line(self.current()),
            );
            return None;
        }

        self.advance();
        self.skip_whitespace();

        let log_expr = self.parse_expression(0);
        self.skip_whitespace();

        if self.check_symbol(';') {
            self.advance();
        }

        Some(QuickFuncStatement::Log {
            value: log_expr,
            position,
        })
    }

    /// Parse variable declaration
    /// Syntax: let [mut] identifier [<type>] = expression
    fn parse_variable_declaration(
        &mut self,
        decl_type: DeclarationType,
        start_position: Position
    ) -> QuickFuncStatement {
        self.advance();
        self.skip_whitespace();

        // Check for 'mut' modifier
        let is_mutable = if decl_type == DeclarationType::Let {
            if let TokenType::Keyword(kw) = &self.current().token_type {
                if kw.as_str() == "mut" {
                    self.advance();
                    self.skip_whitespace();
                    self.log_verbose("Parsed 'mut' modifier");
                    true
                } else {
                    false
                }
            } else {
                false
            }
        } else {
            // Check for invalid mut on const
            if let TokenType::Keyword(kw) = &self.current().token_type {
                if kw.as_str() == "mut" {
                    self.error_manager.add_parse_error(
                        ParseErrorType::InvalidOperation,
                        "'const' declarations cannot be mutable - remove 'mut' or use 'let'".to_string(),
                        self.current().line,
                        self.current().column,
                        None,
                        self.get_source_line(self.current()),
                    );
                    self.advance();
                    self.skip_whitespace();
                }
            }
            false
        };

        // Parse variable name
        let var_name = match &self.current().token_type {
            TokenType::Identifier(id) => {
                let name = id.clone();
                self.advance();
                Some(name)
            }
            TokenType::Keyword(kw)
            if Keywords::can_be_identifier_in_context(kw, "QUICKFUNCS") =>
                {
                    let name = kw.clone();  // FIX: Clone before advancing
                    self.advance();
                    self.log_verbose(&format!("Accepted keyword '{}' as variable name", name));
                    Some(name)
                }
            _ => {
                self.error_manager.add_parse_error(
                    ParseErrorType::MissingToken,
                    format!("Expected variable name after '{}'",
                            if decl_type == DeclarationType::Let { "let" } else { "const" }),
                    self.current().line,
                    self.current().column,
                    None,
                    self.get_source_line(self.current()),
                );
                None
            }
        };

        let var_name = var_name.unwrap_or_else(|| "unknown".to_string());
        self.skip_whitespace();

        // Parse type annotation
        let var_type = if self.check_symbol('<') {
            self.parse_type_annotation()
        } else {
            None
        };

        self.skip_whitespace();

        // Expect '='
        if !self.check_symbol('=') {
            self.error_manager.add_parse_error(
                ParseErrorType::MissingToken,
                format!("Expected '=' after variable declaration '{}'", var_name),
                self.current().line,
                self.current().column,
                None,
                self.get_source_line(self.current()),
            );

            return QuickFuncStatement::ExpressionStatement {
                expression: Expression::Value {
                    value: Value::Null { position: start_position },
                    position: start_position,
                },
                position: start_position,
            };
        }

        self.advance();
        self.skip_whitespace();

        let init_expr = self.parse_expression(0);
        self.skip_whitespace();

        if self.check_symbol(';') {
            self.advance();
        }

        QuickFuncStatement::VariableDeclaration {
            declaration_type: decl_type,
            is_mutable,
            variable_name: var_name,
            data_type: var_type,
            value: init_expr,
            position: start_position,
        }
    }

    /// Parse if statement
    /// Syntax: if: condition { statements } elif: condition { statements } else { statements }
    fn parse_if_statement(&mut self) -> Option<QuickFuncStatement> {
        let if_start_token = self.current().clone();
        let if_position = Position::from_token(&if_start_token);

        self.advance();

        if !self.expect_symbol(':') {
            return Some(QuickFuncStatement::If {
                condition: Expression::Value {
                    value: Value::Boolean { value: false, position: if_position },
                    position: if_position,
                },
                then_branch: Vec::new(),
                else_branch: None,
                position: if_position,
            });
        }

        self.skip_whitespace();
        let condition = self.parse_expression(0);
        self.skip_whitespace();

        // Check for single-line syntax
        let is_single_line = if let TokenType::Keyword(kw) = &self.current().token_type {
            kw.as_str() == "then"
        } else {
            false
        };

        let then_branch = if is_single_line {
            self.advance();
            self.skip_whitespace();
            if let Some(stmt) = self.parse_statement() {
                vec![stmt]
            } else {
                Vec::new()
            }
        } else {
            if !self.check_symbol('{') {
                self.error_manager.add_parse_error(
                    ParseErrorType::MissingToken,
                    "Expected '{' or 'then' after if condition".to_string(),
                    self.current().line,
                    self.current().column,
                    None,
                    self.get_source_line(self.current()),
                );
                return Some(QuickFuncStatement::If {
                    condition,
                    then_branch: Vec::new(),
                    else_branch: None,
                    position: if_position,
                });
            }
            self.parse_statement_block()
        };

        self.skip_whitespace();

        // Parse elif chain
        let mut elif_chain = Vec::new();

        while !self.is_at_end() {
            if let TokenType::Keyword(kw) = &self.current().token_type {
                if kw.as_str() != "elif" {  // FIX: Use .as_str()
                    break;
                }
            } else {
                break;
            }

            let elif_position = Position::from_token(self.current());
            self.advance();
            self.skip_whitespace();

            if !self.expect_symbol(':') {
                break;
            }

            self.skip_whitespace();
            let elif_condition = self.parse_expression(0);
            self.skip_whitespace();

            if !self.check_symbol('{') {
                self.error_manager.add_parse_error(
                    ParseErrorType::MissingToken,
                    "Expected '{' after elif condition".to_string(),
                    self.current().line,
                    self.current().column,
                    None,
                    self.get_source_line(self.current()),
                );
                break;
            }

            let elif_body = self.parse_statement_block();
            self.skip_whitespace();

            elif_chain.push(QuickFuncStatement::If {
                condition: elif_condition,
                then_branch: elif_body,
                else_branch: None,
                position: elif_position,
            });
        }

        // Parse else branch
        let mut final_else_branch = None;
        if !self.is_at_end() {
            if let TokenType::Keyword(kw) = &self.current().token_type {
                if kw.as_str() == "else" {
                    self.advance();
                    self.skip_whitespace();

                    if !self.check_symbol('{') {
                        self.error_manager.add_parse_error(
                            ParseErrorType::MissingToken,
                            "Expected '{' after else".to_string(),
                            self.current().line,
                            self.current().column,
                            None,
                            self.get_source_line(self.current()),
                        );
                    } else {
                        final_else_branch = Some(self.parse_statement_block());
                    }
                }
            }
        }

        // Build elif chain from bottom up
        let mut current_else_branch = final_else_branch;

        for elif in elif_chain.into_iter().rev() {
            // FIX: Destructure once instead of multiple pattern matches
            if let QuickFuncStatement::If { condition, then_branch, position, .. } = elif {
                let elif_with_else = QuickFuncStatement::If {
                    condition,
                    then_branch,
                    else_branch: current_else_branch,
                    position,
                };
                current_else_branch = Some(vec![elif_with_else]);
            }
        }

        Some(QuickFuncStatement::If {
            condition,
            then_branch,
            else_branch: current_else_branch,
            position: if_position,
        })
    }

    /// Parse switch statement
    /// Syntax: chk: expression { -> case1 { statements } -> miss { statements } }
    fn parse_switch_statement(&mut self) -> Option<QuickFuncStatement> {
        let switch_position = Position::from_token(self.current());

        self.advance();

        if !self.expect_symbol(':') {
            return Some(QuickFuncStatement::Switch {
                expression: Expression::Value {
                    value: Value::Null { position: switch_position },
                    position: switch_position,
                },
                cases: Vec::new(),
                default_case: None,
                position: switch_position,
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
                position: switch_position,
            });
        }

        let mut cases = Vec::new();
        let mut default_case = None;

        while !self.is_at_end() && !self.check_symbol('}') {
            self.skip_whitespace();
            if self.check_symbol('}') {
                break;
            }

            let case_position = Position::from_token(self.current());

            // Expect '->'
            let found_case_arrow = self.match_arrow();

            if !found_case_arrow {
                self.error_manager.add_parse_error(
                    ParseErrorType::UnexpectedToken,
                    "Expected '->' to start switch case".to_string(),
                    self.current().line,
                    self.current().column,
                    None,
                    self.get_source_line(self.current()),
                );
                self.advance();
                continue;
            }

            self.skip_whitespace();

            // Check for miss (default case)
            if let TokenType::Keyword(kw) = &self.current().token_type {
                if kw.as_str() == "miss" {
                    self.advance();
                    self.skip_whitespace();

                    let default_stmts = self.parse_case_body();
                    default_case = Some(SwitchCase::new(
                        Value::Null { position: case_position },
                        default_stmts,
                        case_position,
                    ));

                    self.skip_whitespace();
                    continue;
                }
            }

            let case_value = self.parse_case_value();
            self.skip_whitespace();

            let case_stmts = self.parse_case_body();
            cases.push(SwitchCase::new(case_value, case_stmts, case_position));

            self.skip_whitespace();
        }

        if !self.expect_symbol('}') {
            return Some(QuickFuncStatement::Switch {
                expression: expr,
                cases: Vec::new(),
                default_case: None,
                position: switch_position,
            });
        }

        Some(QuickFuncStatement::Switch {
            expression: expr,
            cases,
            default_case,
            position: switch_position,
        })
    }

    fn parse_case_value(&mut self) -> Value {
        self.parse_value()
    }

    fn parse_case_body(&mut self) -> Vec<QuickFuncStatement> {
        let mut statements = Vec::new();

        self.skip_whitespace();

        // Check for 'then' keyword
        if let TokenType::Keyword(kw) = &self.current().token_type {
            if kw.as_str() == "then" {
                self.advance();
                self.skip_whitespace();

                if let Some(stmt) = self.parse_statement() {
                    statements.push(stmt);
                }

                return statements;
            }
        }

        // Check for '=>' arrow
        if self.check_arrow() {
            self.advance();
            self.skip_whitespace();

            if let Some(stmt) = self.parse_statement() {
                statements.push(stmt);
            }

            return statements;
        }

        // Check for block
        if self.check_symbol('{') {
            return self.parse_statement_block();
        }

        self.error_manager.add_parse_error(
            ParseErrorType::MissingToken,
            "Expected 'then', '=>', or '{' after switch case value".to_string(),
            self.current().line,
            self.current().column,
            None,
            self.get_source_line(self.current()),
        );

        statements
    }

    // ==================== PRATT EXPRESSION PARSING ====================

    /// Parse expression using Pratt's algorithm
    /// All dotted identifiers become QualifiedIdentifier - analyzer resolves them
    fn parse_expression(&mut self, min_precedence: i32) -> Expression {
        self.skip_whitespace();

        if self.is_at_end() {
            self.log_verbose("ParseExpression: At end of tokens");
            return Expression::Value {
                value: Value::Null { position: Position::UNKNOWN },
                position: Position::UNKNOWN,
            };
        }

        self.log_verbose(&format!("ParseExpression: Parsing left operand at position {}",
                                  self.position));

        let mut left = self.parse_unary_or_primary();
        self.skip_whitespace();

        while !self.is_at_end() {
            let current_token = self.current().clone();

            // Check for ternary operator
            if let TokenType::Symbol('?') = current_token.token_type {
                let ternary_precedence = 2;

                if ternary_precedence < min_precedence {
                    break;
                }

                let ternary_position = Position::from_token(&current_token);
                self.advance();
                self.skip_whitespace();

                let true_branch = self.parse_expression(2);
                self.skip_whitespace();

                if !self.check_symbol(':') {
                    self.error_manager.add_parse_error(
                        ParseErrorType::MissingToken,
                        "Expected ':' in ternary expression".to_string(),
                        self.current().line,
                        self.current().column,
                        None,
                        self.get_source_line(self.current()),
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
                    position: ternary_position,
                };

                self.skip_whitespace();
                continue;
            }

            // Try to get operator info
            let (op, prec, right_assoc) = match self.try_get_operator_precedence(&current_token) {
                Some(info) => info,
                None => break,
            };

            if prec < min_precedence {
                break;
            }

            let op_position = Position::from_token(&current_token);
            self.advance();
            self.skip_whitespace();

            let next_min_prec = if right_assoc { prec } else { prec + 1 };
            let right = self.parse_expression(next_min_prec);

            left = self.create_binary_expression(left, &op, right, op_position);
            self.skip_whitespace();
        }

        left
    }

    fn try_get_operator_precedence(&self, token: &Token) -> Option<(String, i32, bool)> {
        match &token.token_type {
            TokenType::ArithmeticOp(op) => {
                OPERATOR_PRECEDENCE.get(op.as_str()).map(|&(prec, ra)| {
                    (op.clone(), prec, ra)
                })
            }
            TokenType::BitwiseOp(op) => {
                if op.ends_with('=') || op.as_str() == "~?" {
                    return None;
                }
                OPERATOR_PRECEDENCE.get(op.as_str()).map(|&(prec, ra)| {
                    (op.clone(), prec, ra)
                })
            }
            TokenType::ComparisonOp(op) => {
                OPERATOR_PRECEDENCE.get(op.as_str()).map(|&(prec, ra)| {
                    (op.clone(), prec, ra)
                })
            }
            TokenType::LogicalOp(op) => {
                OPERATOR_PRECEDENCE.get(op.as_str()).map(|&(prec, ra)| {
                    (op.clone(), prec, ra)
                })
            }
            TokenType::Symbol(sym) => {
                let sym_str = sym.to_string();
                OPERATOR_PRECEDENCE.get(sym_str.as_str()).map(|&(prec, ra)| {
                    (sym_str, prec, ra)
                })
            }
            TokenType::Keyword(kw) => {
                let kw_lower = kw.to_lowercase();
                OPERATOR_PRECEDENCE.get(kw_lower.as_str()).map(|&(prec, ra)| {
                    (kw_lower, prec, ra)
                })
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
            "+" | "-" | "*" | "/" | "%" | "**" | "%%" | "%&" | "&%" => {
                Expression::ArithmeticOp {
                    left: Box::new(left),
                    operator: op.to_string(),
                    right: Box::new(right),
                    position,
                }
            }
            ">" | "<" | ">=" | "<=" | "==" | "!=" => {
                Expression::ComparisonOp {
                    left: Box::new(left),
                    operator: op.to_string(),
                    right: Box::new(right),
                    position,
                }
            }
            "&&" | "||" | "and" | "or" => {
                Expression::LogicalOp {
                    left: Box::new(left),
                    operator: op.to_string(),
                    right: Box::new(right),
                    position,
                }
            }
            "&" | "|" | "^" | "<<" | ">>" => {
                Expression::BitwiseOp {
                    left: Box::new(left),
                    operator: op.to_string(),
                    right: Box::new(right),
                    position,
                }
            }
            _ => Expression::ArithmeticOp {
                left: Box::new(left),
                operator: op.to_string(),
                right: Box::new(right),
                position,
            },
        }
    }

    /// Parse unary operators and primary expressions
    fn parse_unary_or_primary(&mut self) -> Expression {
        self.skip_whitespace();
        if self.is_at_end() {
            return Expression::Value {
                value: Value::Null { position: Position::UNKNOWN },
                position: Position::UNKNOWN,
            };
        }

        let token = self.current().clone();
        let unary_position = Position::from_token(&token);

        // Check for unary operators
        let unary_op = match &token.token_type {
            TokenType::Symbol(sym) if VALID_UNARY_OPERATORS.contains(&sym.to_string().as_str()) => {
                Some(sym.to_string())
            }
            TokenType::ArithmeticOp(op) if op.as_str() == "+" || op.as_str() == "-" => {
                Some(op.clone())
            }
            TokenType::Keyword(kw) if VALID_UNARY_OPERATORS.contains(&kw.as_str()) => {
                Some(kw.clone())
            }
            TokenType::BitwiseOp(op) if op.as_str() == "~?" => {
                Some(op.clone())
            }
            _ => None,
        };

        if let Some(op) = unary_op {
            self.advance();
            self.skip_whitespace();

            let operand = self.parse_primary_base();

            let unary_expr = Expression::UnaryOp {
                operator: op,
                operand: Box::new(operand),
                position: unary_position,
            };

            return self.apply_postfix_operations(unary_expr);
        }

        self.parse_primary_with_postfix()
    }

    /// Apply postfix operations - creates QualifiedIdentifier for all dotted patterns
    fn apply_postfix_operations(&mut self, mut expr: Expression) -> Expression {
        let mut parts = Vec::new();

        if let Expression::Identifier { name, .. } = &expr {
            parts.push(name.clone());
        }

        while !self.is_at_end() {
            self.skip_whitespace();

            let token = self.current().clone();

            if let TokenType::Symbol('.') = token.token_type {
                let dot_position = Position::from_token(&token);
                self.advance();
                self.skip_whitespace();

                let member_name = match &self.current().token_type {
                    TokenType::Identifier(id) => {
                        let name = id.clone();
                        self.advance();
                        Some(name)
                    }
                    TokenType::Keyword(kw)
                    if Keywords::can_be_identifier_in_context(kw, "QUICKFUNCS") =>
                        {
                            let name = kw.clone();  // FIX: Clone before advancing
                            self.advance();
                            self.log_verbose(&format!("Accepted keyword '{}' as member name after '.'", name));
                            Some(name)
                        }
                    _ => {
                        self.error_manager.add_parse_error(
                            ParseErrorType::UnexpectedToken,
                            "Expected identifier or data type keyword after '.'".to_string(),
                            self.current().line,
                            self.current().column,
                            None,
                            self.get_source_line(self.current()),
                        );
                        None
                    }
                };

                if member_name.is_none() {
                    break;
                }

                let member = member_name.unwrap();
                self.skip_whitespace();

                if !parts.is_empty() {
                    parts.push(member);
                } else {
                    expr = Expression::PropertyAccess {
                        object: Box::new(expr),
                        property: member,
                        position: dot_position,
                    };
                }
            } else if let TokenType::Symbol('[') = token.token_type {
                let bracket_position = Position::from_token(&token);
                self.advance();
                self.skip_whitespace();

                let index_expr = self.parse_expression(0);
                self.skip_whitespace();

                if !self.expect_symbol(']') {
                    break;
                }

                expr = Expression::IndexAccess {
                    object: Box::new(expr),
                    index: Box::new(index_expr),
                    position: bracket_position,
                };

                parts.clear();
            } else {
                break;
            }
        }

        // If we collected multiple parts, create QualifiedIdentifier
        if parts.len() >= 2 {
            let position = expr.position();

            self.skip_whitespace();
            if self.check_symbol('(') {
                let args = self.parse_function_arguments();

                return Expression::QualifiedIdentifier {
                    parts,
                    arguments: Some(args),
                    position,
                };
            }

            return Expression::QualifiedIdentifier {
                parts,
                arguments: None,
                position,
            };
        }

        expr
    }

    fn parse_primary_with_postfix(&mut self) -> Expression {
        self.skip_whitespace();
        if self.is_at_end() {
            return Expression::Value {
                value: Value::Null { position: Position::UNKNOWN },
                position: Position::UNKNOWN,
            };
        }

        let expr = self.parse_primary_base();
        self.apply_postfix_operations(expr)
    }

    /// Parse primary expressions - simplified, no identifier resolution
    fn parse_primary_base(&mut self) -> Expression {
        self.skip_whitespace();
        if self.is_at_end() {
            return Expression::Value {
                value: Value::Null { position: Position::UNKNOWN },
                position: Position::UNKNOWN,
            };
        }

        let token = self.current().clone();
        let token_position = Position::from_token(&token);

        match &token.token_type {
            TokenType::Integer(i) => {
                let val = *i;
                self.advance();
                Expression::Value {
                    value: Value::Integer { value: val, position: token_position },
                    position: token_position,
                }
            }
            TokenType::Float(f) => {
                let val = *f;
                self.advance();
                Expression::Value {
                    value: Value::Float { value: val, position: token_position },
                    position: token_position,
                }
            }
            TokenType::Double(d) => {
                let val = *d;
                self.advance();
                Expression::Value {
                    value: Value::Double { value: val, position: token_position },
                    position: token_position,
                }
            }
            TokenType::String(s) => {
                let val = s.clone();
                self.advance();
                Expression::Value {
                    value: Value::String { value: val, position: token_position },
                    position: token_position,
                }
            }
            TokenType::InterpolatedString(template) => {
                let raw_content = template.clone();
                self.advance();

                let (final_template, expressions) = self.parse_interpolated_string_content(&raw_content, token_position);

                Expression::Value {
                    value: Value::InterpolatedString {
                        template: final_template,
                        expressions,
                        position: token_position,
                    },
                    position: token_position,
                }
            }
            TokenType::Bool(b) => {
                let val = *b;
                self.advance();
                Expression::Value {
                    value: Value::Boolean { value: val, position: token_position },
                    position: token_position,
                }
            }
            TokenType::Identifier(id) => {
                let name = id.clone();
                let identifier_position = Position::from_token(&token);

                self.advance();
                self.skip_whitespace();

                if self.check_symbol('(') {
                    let args = self.parse_function_arguments();
                    return Expression::QuickFuncCall {
                        name,
                        arguments: args,
                        position: identifier_position,
                    };
                }

                Expression::Identifier {
                    name,
                    position: identifier_position,
                }
            }
            TokenType::Symbol('(') => {
                let saved_position = self.position;

                if self.is_lambda_expression() {
                    return self.parse_lambda_expression();
                }

                self.position = saved_position;

                self.advance();
                self.skip_whitespace();
                let expr = self.parse_expression(0);
                self.skip_whitespace();

                if !self.expect_symbol(')') {
                    return Expression::Value {
                        value: Value::Null { position: token_position },
                        position: token_position,
                    };
                }

                Expression::Parenthesized {
                    expression: Box::new(expr),
                    position: token_position,
                }
            }
            TokenType::Symbol('[') => {
                let arr = self.parse_array_literal();
                Expression::Value {
                    value: arr,
                    position: token_position,
                }
            }
            TokenType::Symbol('{') => {
                let obj = self.parse_object_literal();
                Expression::Value {
                    value: obj,
                    position: token_position,
                }
            }
            TokenType::TupleConstructor(_) => self.parse_tuple_constructor(),
            TokenType::BlobConstructor(_) => self.parse_blob_constructor(),
            TokenType::RegexConstructor(_) => self.parse_regex_constructor(),
            _ => {
                self.log_verbose(&format!("Unexpected token in primary expression: {}",
                                          token.get_token_value()));
                self.advance();
                Expression::Value {
                    value: Value::Null { position: token_position },
                    position: token_position,
                }
            }
        }
    }

    // ==================== VALUE AND LITERAL PARSING ====================

    fn parse_value(&mut self) -> Value {
        self.skip_whitespace();
        if self.is_at_end() {
            return Value::Null { position: Position::UNKNOWN };
        }

        let token = self.current().clone();
        let value_position = Position::from_token(&token);

        match &token.token_type {
            TokenType::Integer(i) => {
                let val = *i;
                self.advance();
                Value::Integer { value: val, position: value_position }
            }
            TokenType::Float(f) => {
                let val = *f;
                self.advance();
                Value::Float { value: val, position: value_position }
            }
            TokenType::Double(d) => {
                let val = *d;
                self.advance();
                Value::Double { value: val, position: value_position }
            }
            TokenType::String(s) => {
                let val = s.clone();
                self.advance();
                Value::String { value: val, position: value_position }
            }
            TokenType::Bool(b) => {
                let val = *b;
                self.advance();
                Value::Boolean { value: val, position: value_position }
            }
            TokenType::Keyword(kw) if kw.as_str() == "null" => {
                self.advance();
                Value::Null { position: value_position }
            }
            TokenType::Identifier(id) => {
                let val = id.clone();
                self.advance();
                Value::Identifier { value: val, position: value_position }
            }
            TokenType::Symbol('[') => self.parse_array_literal(),
            TokenType::Symbol('{') => self.parse_object_literal(),
            TokenType::HexColor(hc) => {
                let val = hc.clone();
                self.advance();
                Value::HexColor { value: val, position: value_position }
            }
            TokenType::Date(d) => {
                let val = d.clone();
                self.advance();
                Value::Date { value: val, position: value_position }
            }
            TokenType::Timestamp(t) => {
                let val = t.clone();
                self.advance();
                Value::Timestamp { value: val, position: value_position }
            }
            TokenType::InterpolatedString(template) => {
                let raw_content = template.clone();
                self.advance();

                let (final_template, expressions) = self.parse_interpolated_string_content(&raw_content, value_position);

                Value::InterpolatedString {
                    template: final_template,
                    expressions,
                    position: value_position,
                }
            }
            _ => {
                self.log_verbose(&format!("ParseValue: Unexpected token type {}, treating as identifier",
                                          token.get_token_value()));
                let val = token.get_token_value();
                self.advance();
                Value::Identifier { value: val, position: value_position }
            }
        }
    }

    fn parse_array_literal(&mut self) -> Value {
        let array_position = Position::from_token(self.current());

        if !self.expect_symbol('[') {
            return Value::Array {
                values: Vec::new(),
                position: array_position,
            };
        }

        let estimated_items = usize::max(4, self.tokens.len() / 40);
        let mut items = Vec::with_capacity(estimated_items);

        while !self.is_at_end() && !self.check_symbol(']') {
            self.skip_whitespace();
            if self.check_symbol(']') {
                break;
            }

            let expr = self.parse_expression(0);

            let item = match expr {
                Expression::Value { value, .. } => value,
                _ => Value::Expression {
                    expr: Box::new(expr),
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

        if self.check_symbol(']') {
            self.advance();
        }

        self.log_verbose(&format!("Parsed array with {} item(s)", items.len()));
        Value::Array {
            values: items,
            position: array_position,
        }
    }

    fn parse_object_literal(&mut self) -> Value {
        let object_position = Position::from_token(self.current());
        self.log_verbose(&format!("ParseObjectLiteral starting at position {}", self.position));

        if !self.expect_symbol('{') {
            return Value::Object {
                properties: Vec::new(),
                position: object_position,
            };
        }

        let estimated_props = estimate_properties_count(self.tokens.len());
        let mut properties = Vec::with_capacity(estimated_props);

        while !self.is_at_end() && !self.check_symbol('}') {
            self.skip_whitespace();
            if self.check_symbol('}') {
                break;
            }

            let prop_position = Position::from_token(self.current());

            // Parse property name
            let property_name = match &self.current().token_type {
                TokenType::Identifier(id) => {
                    let name = id.clone();
                    self.advance();
                    Some(name)
                }
                TokenType::Keyword(kw)
                if Keywords::can_be_identifier_in_context(kw, "QUICKFUNCS") =>
                    {
                        let name = kw.clone();
                        self.advance();
                        Some(name)
                    }
                TokenType::String(s) => {
                    let name = s.clone();
                    self.advance();
                    Some(name)
                }
                _ => {
                    self.error_manager.add_parse_error(
                        ParseErrorType::UnexpectedToken,
                        format!("Expected property name, found {}",
                                self.current().get_token_value()),
                        self.current().line,
                        self.current().column,
                        None,
                        self.get_source_line(self.current()),
                    );
                    self.advance();
                    None
                }
            };

            if property_name.is_none() {
                continue;
            }

            let prop_name = property_name.unwrap();
            self.skip_whitespace();

            // Expect ':' or '='
            if !self.check_symbol(':') && !self.check_symbol('=') {
                self.error_manager.add_parse_error(
                    ParseErrorType::MissingToken,
                    format!("Expected ':' or '=' after property name '{}'", prop_name),
                    self.current().line,
                    self.current().column,
                    None,
                    self.get_source_line(self.current()),
                );

                while !self.is_at_end() && !self.check_symbol(',') && !self.check_symbol('}') {
                    self.advance();
                }
                continue;
            }

            self.advance();
            self.skip_whitespace();

            let value_expression = self.parse_expression(0);
            let property_value = self.convert_expression_to_value(value_expression);

            properties.push(ObjectProperty::new(prop_name, property_value, prop_position));

            self.skip_whitespace();

            if self.check_symbol(',') {
                self.advance();
                self.skip_whitespace();

                if self.check_symbol('}') {
                    break;
                }
            } else if self.check_symbol('}') {
                break;
            } else {
                self.error_manager.add_parse_error(
                    ParseErrorType::MissingToken,
                    format!("Expected ',' or '}}' in object literal, found {}",
                            self.current().get_token_value()),
                    self.current().line,
                    self.current().column,
                    None,
                    self.get_source_line(self.current()),
                );
                break;
            }
        }

        if self.check_symbol('}') {
            self.advance();
        }

        Value::Object {
            properties,
            position: object_position,
        }
    }

    fn parse_interpolated_string_content(
        &self,
        raw_content: &str,
        position: Position,
    ) -> (String, Vec<Expression>) {
        use regex::Regex;

        let mut expressions = Vec::new();
        let mut template = String::new();
        let mut expression_index = 0;

        // Simple pattern matching for {expr}
        let re = Regex::new(r"\{([^}]+)\}").unwrap();
        let mut last_end = 0;

        for cap in re.captures_iter(raw_content) {
            let match_start = cap.get(0).unwrap().start();
            let match_end = cap.get(0).unwrap().end();
            let expr_text = cap.get(1).unwrap().as_str();

            // Add literal text before this expression
            template.push_str(&raw_content[last_end..match_start]);

            // Parse the expression content
            let parsed_expr = self.parse_interpolated_expression(expr_text, position);
            expressions.push(parsed_expr);

            // Add placeholder
            template.push_str(&format!("{{{}}}", expression_index));
            expression_index += 1;

            last_end = match_end;
        }

        // Add remaining literal text
        template.push_str(&raw_content[last_end..]);

        (template, expressions)
    }

    fn parse_interpolated_expression(&self, expr_text: &str, position: Position) -> Expression {
        let trimmed = expr_text.trim();

        // Try parsing as integer
        if let Ok(val) = trimmed.parse::<i32>() {
            return Expression::Value {
                value: Value::Integer { value: val, position },
                position,
            };
        }

        // Try parsing as float (with 'f' suffix)
        if trimmed.ends_with('f') || trimmed.ends_with('F') {
            let num_part = &trimmed[..trimmed.len() - 1];
            if let Ok(val) = num_part.parse::<f32>() {
                return Expression::Value {
                    value: Value::Float { value: val, position },
                    position,
                };
            }
        }

        // Try parsing as double
        if trimmed.contains('.') {
            if let Ok(val) = trimmed.parse::<f64>() {
                return Expression::Value {
                    value: Value::Double { value: val, position },
                    position,
                };
            }
        }

        // Try parsing as boolean
        if trimmed.eq_ignore_ascii_case("true") {
            return Expression::Value {
                value: Value::Boolean { value: true, position },
                position,
            };
        }
        if trimmed.eq_ignore_ascii_case("false") {
            return Expression::Value {
                value: Value::Boolean { value: false, position },
                position,
            };
        }

        // Try parsing as quoted string
        if (trimmed.starts_with('"') && trimmed.ends_with('"'))
            || (trimmed.starts_with('\'') && trimmed.ends_with('\''))
        {
            let string_content = &trimmed[1..trimmed.len() - 1];
            return Expression::Value {
                value: Value::String {
                    value: string_content.to_string(),
                    position,
                },
                position,
            };
        }

        // Check for method call: obj.method()
        use regex::Regex;
        let method_re = Regex::new(r"^(\w+)\.(\w+)\(\)$").unwrap();
        if let Some(cap) = method_re.captures(trimmed) {
            let parts = vec![cap[1].to_string(), cap[2].to_string()];
            return Expression::QualifiedIdentifier {
                parts,
                arguments: Some(Vec::new()),
                position,
            };
        }

        // Check for property access: obj.property
        let prop_re = Regex::new(r"^(\w+)\.(\w+)$").unwrap();
        if let Some(cap) = prop_re.captures(trimmed) {
            let parts = vec![cap[1].to_string(), cap[2].to_string()];
            return Expression::QualifiedIdentifier {
                parts,
                arguments: None,
                position,
            };
        }

        // Default: treat as identifier
        Expression::Identifier {
            name: trimmed.to_string(),
            position,
        }
    }

    fn parse_tuple_constructor(&mut self) -> Expression {
        let tuple_position = Position::from_token(self.current());
        self.advance();
        self.skip_whitespace();

        if !self.expect_symbol('(') {
            return Expression::Value {
                value: Value::Null { position: tuple_position },
                position: tuple_position,
            };
        }

        let mut tuple_expressions = Vec::new();

        while !self.is_at_end() && !self.check_symbol(')') {
            self.skip_whitespace();
            if self.check_symbol(')') {
                break;
            }

            let expr = self.parse_expression(0);
            tuple_expressions.push(Value::Expression {
                expr: Box::new(expr),
                position: Position::from_token(self.current()),
            });

            self.skip_whitespace();

            if self.check_symbol(',') {
                self.advance();
            } else {
                break;
            }
        }

        if !self.expect_symbol(')') {
            return Expression::Value {
                value: Value::Null { position: tuple_position },
                position: tuple_position,
            };
        }

        Expression::Value {
            value: Value::PrefixedConstructor {
                prefix: "t".to_string(),
                arguments: tuple_expressions,
                position: tuple_position,
            },
            position: tuple_position,
        }
    }

    fn parse_blob_constructor(&mut self) -> Expression {
        let blob_position = Position::from_token(self.current());
        self.advance();
        self.skip_whitespace();

        if !self.expect_symbol('(') {
            return Expression::Value {
                value: Value::Null { position: blob_position },
                position: blob_position,
            };
        }

        let _blob_value = self.parse_expression(0);
        self.skip_whitespace();

        if !self.expect_symbol(')') {
            return Expression::Value {
                value: Value::Null { position: blob_position },
                position: blob_position,
            };
        }

        let blob_args = vec![Value::String {
            value: "blob_data".to_string(),
            position: blob_position,
        }];

        Expression::Value {
            value: Value::PrefixedConstructor {
                prefix: "b".to_string(),
                arguments: blob_args,
                position: blob_position,
            },
            position: blob_position,
        }
    }

    fn parse_regex_constructor(&mut self) -> Expression {
        let regex_position = Position::from_token(self.current());
        self.advance();
        self.skip_whitespace();

        if !self.expect_symbol('(') {
            return Expression::Value {
                value: Value::Null { position: regex_position },
                position: regex_position,
            };
        }

        let _regex_pattern = self.parse_expression(0);
        self.skip_whitespace();

        if !self.expect_symbol(')') {
            return Expression::Value {
                value: Value::Null { position: regex_position },
                position: regex_position,
            };
        }

        let regex_args = vec![Value::String {
            value: "regex_pattern".to_string(),
            position: regex_position,
        }];

        Expression::Value {
            value: Value::PrefixedConstructor {
                prefix: "r".to_string(),
                arguments: regex_args,
                position: regex_position,
            },
            position: regex_position,
        }
    }

    fn parse_function_arguments(&mut self) -> Vec<Expression> {
        let estimated_args = usize::max(2, self.tokens.len() / 50);
        let mut arguments = Vec::with_capacity(estimated_args);

        if !self.expect_symbol('(') {
            return arguments;
        }

        self.skip_whitespace();

        if self.check_symbol(')') {
            self.advance();
            return arguments;
        }

        loop {
            self.skip_whitespace();

            let arg = self.parse_expression(0);
            arguments.push(arg);

            self.skip_whitespace();

            if self.check_symbol(',') {
                self.advance();
                self.skip_whitespace();

                if self.check_symbol(')') {
                    self.error_manager.add_parse_error(
                        ParseErrorType::UnexpectedToken,
                        "Trailing comma in function arguments".to_string(),
                        self.current().line,
                        self.current().column,
                        None,
                        self.get_source_line(self.current()),
                    );
                    break;
                }
            } else {
                break;
            }
        }

        if !self.expect_symbol(')') {
            self.error_manager.add_parse_error(
                ParseErrorType::MissingToken,
                "Expected ')' after function arguments".to_string(),
                self.current().line,
                self.current().column,
                None,
                self.get_source_line(self.current()),
            );
        }

        arguments
    }

    fn convert_expression_to_value(&self, expr: Expression) -> Value {
        let expr_position = expr.position();

        match expr {
            Expression::Value { value, .. } => value,
            Expression::Identifier { name, .. } => Value::Identifier {
                value: name,
                position: expr_position,
            },
            Expression::QualifiedIdentifier { parts, .. } => Value::Identifier {
                value: parts.join("."),
                position: expr_position,
            },
            _ => Value::Expression {
                expr: Box::new(expr),
                position: expr_position,
            },
        }
    }

    // ==================== LAMBDA EXPRESSION PARSING ====================

    fn is_lambda_expression(&self) -> bool {
        if !self.check_symbol('(') {
            return false;
        }

        let mut look_ahead = self.position;
        let mut paren_depth = 0;

        while look_ahead < self.tokens.len() {
            let token = &self.tokens[look_ahead];

            if let TokenType::Symbol(sym) = token.token_type {
                if sym == '(' {
                    paren_depth += 1;
                } else if sym == ')' {
                    paren_depth -= 1;
                    if paren_depth == 0 {
                        look_ahead += 1;

                        while look_ahead < self.tokens.len()
                            && self.tokens[look_ahead].get_token_value().trim().is_empty()
                        {
                            look_ahead += 1;
                        }

                        if look_ahead < self.tokens.len() {
                            let next_token = &self.tokens[look_ahead];
                            return matches!(next_token.token_type, TokenType::Arrow)
                                || matches!(&next_token.token_type, TokenType::Symbol('='));
                        }

                        return false;
                    }
                }
            }

            look_ahead += 1;
        }

        false
    }

    fn parse_lambda_expression(&mut self) -> Expression {
        let lambda_position = Position::from_token(self.current());
        self.log_verbose(&format!("Parsing lambda expression at position {}", self.position));

        let parameters = self.parse_lambda_parameters();

        self.skip_whitespace();

        if !self.check_arrow() {
            if self.check_symbol('=') {
                self.advance();
                if !self.expect_symbol('>') {
                    self.error_manager.add_parse_error(
                        ParseErrorType::MissingToken,
                        "Expected '=>' after lambda parameters".to_string(),
                        self.current().line,
                        self.current().column,
                        None,
                        self.get_source_line(self.current()),
                    );
                    return Expression::Value {
                        value: Value::Null { position: lambda_position },
                        position: lambda_position,
                    };
                }
            } else {
                self.error_manager.add_parse_error(
                    ParseErrorType::MissingToken,
                    "Expected '=>' after lambda parameters".to_string(),
                    self.current().line,
                    self.current().column,
                    None,
                    self.get_source_line(self.current()),
                );
                return Expression::Value {
                    value: Value::Null { position: lambda_position },
                    position: lambda_position,
                };
            }
        } else {
            self.advance();
        }

        self.skip_whitespace();

        let body = self.parse_lambda_body();

        // FIX: Get length before move
        let param_count = parameters.len();

        let lambda_value = Value::Lambda {
            parameters,
            body: Box::new(body),
            position: lambda_position,
        };

        self.log_verbose(&format!("Parsed lambda with {} parameters", param_count));

        Expression::Value {
            value: lambda_value,
            position: lambda_position,
        }
    }

    fn parse_lambda_parameters(&mut self) -> Vec<String> {
        let mut parameters = Vec::new();

        if !self.expect_symbol('(') {
            return parameters;
        }

        self.skip_whitespace();

        if self.check_symbol(')') {
            self.advance();
            self.log_verbose("Lambda: empty parameter list");
            return parameters;
        }

        loop {
            self.skip_whitespace();

            if let TokenType::Identifier(id) = &self.current().token_type {
                let param_name = id.clone();
                self.advance();
                self.skip_whitespace();

                // Skip type annotation if present
                if self.check_symbol('<') {
                    self.advance();
                    self.skip_whitespace();

                    if matches!(self.current().token_type, TokenType::Keyword(_) | TokenType::Identifier(_)) {
                        self.advance();
                    }

                    self.skip_whitespace();
                    if !self.expect_symbol('>') {
                        break;
                    }
                    self.skip_whitespace();
                }

                parameters.push(param_name);
                self.log_verbose(&format!("Lambda parameter: {}", parameters.last().unwrap()));

                if self.check_symbol(',') {
                    self.advance();
                } else {
                    break;
                }
            } else {
                self.error_manager.add_parse_error(
                    ParseErrorType::UnexpectedToken,
                    "Expected parameter name in lambda".to_string(),
                    self.current().line,
                    self.current().column,
                    None,
                    self.get_source_line(self.current()),
                );
                break;
            }
        }

        if !self.expect_symbol(')') {
            self.error_manager.add_parse_error(
                ParseErrorType::MissingToken,
                "Expected ')' after lambda parameters".to_string(),
                self.current().line,
                self.current().column,
                None,
                self.get_source_line(self.current()),
            );
        }

        parameters
    }

    fn parse_lambda_body(&mut self) -> Expression {
        self.skip_whitespace();

        if self.check_symbol('{') {
            return self.parse_lambda_block_body();
        }

        self.log_verbose("Parsing lambda expression body");
        self.parse_expression(0)
    }

    fn parse_lambda_block_body(&mut self) -> Expression {
        let block_position = Position::from_token(self.current());
        self.log_verbose("Parsing lambda block body");

        if !self.expect_symbol('{') {
            return Expression::Value {
                value: Value::Null { position: block_position },
                position: block_position,
            };
        }

        let mut statements = Vec::new();

        while !self.is_at_end() && !self.check_symbol('}') {
            self.skip_whitespace();
            if self.check_symbol('}') {
                break;
            }

            if let Some(stmt) = self.parse_statement() {
                // FIX: Get type name before move
                let type_name = std::any::type_name_of_val(&stmt);
                statements.push(stmt);
                self.log_verbose(&format!("Lambda block: parsed {}", type_name));
            }

            self.skip_whitespace();
        }

        if !self.expect_symbol('}') {
            self.error_manager.add_parse_error(
                ParseErrorType::MissingToken,
                "Expected '}' to close lambda block body".to_string(),
                self.current().line,
                self.current().column,
                None,
                self.get_source_line(self.current()),
            );
        }

        self.log_verbose(&format!("Lambda block complete with {} statements",
                                  statements.len()));

        Expression::Value {
            value: Value::Identifier {
                value: format!("<lambda_block:{}_stmts>", statements.len()),
                position: block_position,
            },
            position: block_position,
        }
    }

    // ==================== HELPER METHODS ====================

    fn parse_type_annotation(&mut self) -> Option<DataType> {
        if !self.check_symbol('<') {
            return None;
        }

        self.advance();
        self.skip_whitespace();

        let type_token = self.current().clone();

        let data_type = match &type_token.token_type {
            TokenType::Keyword(kw) | TokenType::Identifier(kw) => {
                match kw.to_lowercase().as_str() {
                    "int" => Some(DataType::Int),
                    "float" => Some(DataType::Float),
                    "double" => Some(DataType::Double),
                    "string" => Some(DataType::String),
                    "bool" => Some(DataType::Bool),
                    "array" => Some(DataType::Array),
                    "object" => Some(DataType::Object),
                    "tuple" => Some(DataType::Tuple),
                    "hex" => Some(DataType::Hex),
                    "blob" => Some(DataType::Blob),
                    "regex" => Some(DataType::Regex),
                    "date" => Some(DataType::Date),
                    "timestamp" => Some(DataType::Timestamp),
                    "enum" => Some(DataType::Enum),
                    "any" => Some(DataType::Any),
                    _ => None,
                }
            }
            _ => None,
        };

        if data_type.is_some() {
            self.advance();
        } else {
            self.error_manager.add_parse_error(
                ParseErrorType::InvalidType,
                format!("Invalid type annotation: {}", type_token.get_token_value()),
                type_token.line,
                type_token.column,
                None,
                self.get_source_line(&type_token),
            );
            self.advance();
        }

        self.skip_whitespace();

        // Don't return early on error - try to recover
        if !self.check_symbol('>') {
            self.error_manager.add_parse_error(
                ParseErrorType::MissingToken,
                "Expected '>' to close type annotation".to_string(),
                self.current().line,
                self.current().column,
                None,
                self.get_source_line(self.current()),
            );

            // Try to find the '>' and consume it for error recovery
            let mut depth = 1;
            while !self.is_at_end() && depth > 0 {
                if self.check_symbol('<') {
                    depth += 1;
                } else if self.check_symbol('>') {
                    depth -= 1;
                    if depth == 0 {
                        self.advance(); // Consume the '>'
                        break;
                    }
                }
                self.advance();
            }
        } else {
            self.advance(); // Consume the '>'
        }

        if data_type.is_some() {
            self.log_verbose(&format!("Parsed type annotation: {:?}", data_type));
        }

        data_type
    }

    fn parse_dotted_path(&mut self) -> Option<String> {
        let mut path = String::new();

        if let TokenType::Identifier(id) | TokenType::Keyword(id) = &self.current().token_type {
            path = id.clone();
            self.advance();
        } else {
            return None;
        }

        while self.check_symbol('.') {
            self.advance();
            self.skip_whitespace();

            if let TokenType::Identifier(id) = &self.current().token_type {
                path.push('.');
                path.push_str(id);
                self.advance();
            } else if let TokenType::Keyword(kw) = &self.current().token_type {
                if Keywords::can_be_identifier_in_context(kw, "QUICKFUNCS") {
                    path.push('.');
                    path.push_str(kw);
                    self.advance();
                } else {
                    self.error_manager.add_parse_error(
                        ParseErrorType::UnexpectedToken,
                        format!("Cannot use language keyword '{}' in path", kw),
                        self.current().line,
                        self.current().column,
                        None,
                        self.get_source_line(self.current()),
                    );
                    break;
                }
            } else {
                self.error_manager.add_parse_error(
                    ParseErrorType::UnexpectedToken,
                    "Expected identifier after '.' in path".to_string(),
                    self.current().line,
                    self.current().column,
                    None,
                    self.get_source_line(self.current()),
                );
                break;
            }
        }

        Some(path)
    }

    fn match_arrow(&mut self) -> bool {
        if let TokenType::MultiCharSymbol(ms) = &self.current().token_type {
            if ms.as_str() == "->" {
                self.advance();
                return true;
            }
        }

        if matches!(self.current().token_type, TokenType::SwitchCase) {
            self.advance();
            return true;
        }

        if let TokenType::Symbol('-') = self.current().token_type {
            if self.position + 1 < self.tokens.len() {
                if let TokenType::Symbol('>') = self.tokens[self.position + 1].token_type {
                    self.advance();
                    self.advance();
                    return true;
                }
            }
        }

        false
    }

    fn check_arrow(&self) -> bool {
        if let TokenType::MultiCharSymbol(ms) = &self.current().token_type {
            return ms.as_str() == "=>";
        }

        if matches!(self.current().token_type, TokenType::Arrow) {
            return true;
        }

        false
    }

    // ==================== TOKEN NAVIGATION ====================

    #[inline]
    fn current(&self) -> &Token {
        self.tokens.get(self.position).unwrap_or_else(|| {
            static EOF_TOKEN: Token = Token {
                token_type: TokenType::EndOfFile,
                line: 1,
                column: 1,
                section: None,
            };
            &EOF_TOKEN
        })
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
            self.error_manager.add_parse_error(
                ParseErrorType::MissingToken,
                format!("Expected '{}'", symbol),
                self.current().line,
                self.current().column,
                None,
                self.get_source_line(self.current()),
            );
            false
        }
    }

    fn skip_whitespace(&mut self) {
        while !self.is_at_end() {
            let token = self.current();

            if matches!(
                token.token_type,
                TokenType::String(_) | TokenType::StringSingle(_) | TokenType::InterpolatedString(_)
            ) {
                break;
            }

            if matches!(token.token_type, TokenType::Comment(_)) {
                self.advance();
                continue;
            }

            let value = token.get_token_value();

            if value.trim().is_empty() || value == "\n" || value == "\r" || value == "\t" {
                self.advance();
            } else {
                break;
            }
        }
    }

    fn get_source_line(&self, token: &Token) -> Option<String> {
        let line_tokens: Vec<&Token> = self.tokens
            .iter()
            .filter(|t| t.line == token.line)
            .collect();

        if line_tokens.is_empty() {
            return None;
        }

        let mut source_line = String::new();
        let mut current_column = 0;

        for t in line_tokens {
            while current_column < t.column {
                source_line.push(' ');
                current_column += 1;
            }

            let token_value = t.get_token_value();
            source_line.push_str(&token_value);
            current_column += token_value.len();
        }

        Some(source_line)
    }

    // ==================== LOGGING ====================

    fn log_debug(&self, message: &str) {
        if self.operational_settings.debug_mode != DebugMode::Off {
            self.error_manager.log_debug(message);
        }
    }

    fn log_verbose(&self, message: &str) {
        if self.operational_settings.debug_mode == DebugMode::Verbose {
            self.error_manager.log_info(message);
        }
    }
}