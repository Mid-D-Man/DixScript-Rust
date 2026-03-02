

                    value,
                    position,
                } => (declaration_type, is_mutable, variable_name, data_type, value, position),
                _ => return,
            };


        if !Self::is_valid_identifier(variable_name) {
            self.add_error(
                result,
                "QFUNC067",
                "INVALID_VARIABLE_NAME",
                &format!(
                    "Invalid variable name '{}' in function '{}'",
                    variable_name, func.name
                ),
                "Variable names must start with a letter and contain only alphanumeric characters and underscores.",
                *position,
            );
            return;
        }


        if Keywords::is_data_type_keyword(variable_name) {
            let suggestion = format!(
                "Use a different name like 'my{}{}' or '{}Value'",
                variable_name.chars().next().unwrap_or('X').to_uppercase(),
                &variable_name[1..],
                variable_name
            );
            self.add_error(
                result,
                "QFUNC067B",
                "DATA_TYPE_KEYWORD_AS_VARIABLE",
                &format!(
                    "Variable '{}' in function '{}' cannot use a data type keyword as name",
                    variable_name, func.name
                ),
                &suggestion,
                *position,
            );
            return;
        }


        if Keywords::is_reserved_in_context(variable_name, "QUICKFUNCS") {
            self.add_error(
                result,
                "QFUNC068",
                "RESERVED_KEYWORD_AS_VARIABLE",
                &Keywords::get_keyword_usage_error(variable_name, "QUICKFUNCS"),
                &format!("Choose a different name for variable '{}'", variable_name),
                *position,
            );
            return;
        }


        if local_scope.has_variable(variable_name) {
            self.add_error(
                result,
                "QFUNC069",
                "VARIABLE_REDECLARATION",
                &format!(
                    "Variable '{}' already declared in function '{}'",
                    variable_name, func.name
                ),
                "Each variable must be declared only once. Use assignment to change its value.",
                *position,
            );
            return;
        }


        let max_depth = Self::calculate_max_depth(100);
        self.validate_expression(value, func, symbol_table, local_scope, result, max_depth);


        // Infer type once; use for both compatibility check and scope registration.
        let local_variable_types = local_scope.get_all_variable_types();
        let visitor = TypeInferenceVisitor::new(symbol_table, Some(local_variable_types));
        let inferred_type = visitor.infer_type_from_expression(value);


        if let (Some(&declared), Some(inferred)) = (data_type.as_ref(), inferred_type) {
            if !Self::are_types_compatible_strict(inferred, declared) {
                self.add_error(
                    result,
                    "QFUNC071",
                    "VARIABLE_TYPE_MISMATCH",
                    &format!(
                        "Variable '{}' declared as {:?} but assigned value of type {:?}",
                        variable_name, declared, inferred
                    ),
                    &format!(
                        "Change the value to match {:?} or remove the type annotation",
                        declared
                    ),
                    *position,
                );
            }
        }


        let is_const = matches!(declaration_type, DeclarationType::Const) || !is_mutable;
        let effective_type = data_type.or(inferred_type);


        local_scope.add_variable(variable_name.clone(), effective_type, is_const);


        if self.debug_config.is_verbose {
            self.error_manager.log_debug(&format!(
                "Declared {} variable '{}' with type {:?}",
                if is_const { "immutable" } else { "mutable" },
                variable_name,
                effective_type
            ));
        }
        }
// ==================== CONTROL FLOW VALIDATION ====================

    fn validate_if_statement(
        &self,
        condition: &Expression,
        then_branch: &[QuickFuncStatement],
        else_branch: Option<&Vec<QuickFuncStatement>>,
        func: &QuickFunction,
        symbol_table: &SymbolTable,
        local_scope: &mut LocalScopeTracker,
        result: &mut SectionAnalysisResult,
        nesting_depth: usize,
        max_depth: usize,
        return_path: &mut ReturnPathAnalyzer,
    ) {
        self.validate_expression(condition, func, symbol_table, local_scope, result, max_depth);

        let visitor = TypeInferenceVisitor::new(symbol_table, None);
        if let Some(cond_type) = visitor.infer_type_from_expression(condition) {
            if cond_type != DataType::Bool {
                self.add_error(
                    result,
                    "QFUNC016",
                    "NON_BOOLEAN_CONDITION",
                    &format!("If statement condition must be boolean, got {:?}", cond_type),
                    "Use comparison operators (==, !=, >, <, etc.) to create boolean conditions.",
                    condition.position(),
                );
            }
        }

        let mut then_returns = ReturnPathAnalyzer::new(func.return_type.unwrap());
        let mut else_returns = ReturnPathAnalyzer::new(func.return_type.unwrap());

        for stmt in then_branch {
            self.validate_statement(
                stmt, func, symbol_table, local_scope, result,
                nesting_depth + 1, max_depth, &mut then_returns,
            );
        }

        if let Some(else_stmts) = else_branch {
            for stmt in else_stmts {
                self.validate_statement(
                    stmt, func, symbol_table, local_scope, result,
                    nesting_depth + 1, max_depth, &mut else_returns,
                );
            }
            if then_returns.all_paths_return() && else_returns.all_paths_return() {
                return_path.add_return();
            }
        }
    }

    fn validate_switch_statement(
        &self,
        expression: &Expression,
        cases: &[SwitchCase],
        default_case: Option<&SwitchCase>,
        func: &QuickFunction,
        symbol_table: &SymbolTable,
        local_scope: &mut LocalScopeTracker,
        result: &mut SectionAnalysisResult,
        nesting_depth: usize,
        max_depth: usize,
        return_path: &mut ReturnPathAnalyzer,
    ) {
        self.validate_expression(expression, func, symbol_table, local_scope, result, max_depth);

        let has_default = default_case.is_some();
        let all_cases_return = cases.iter().all(|case| {
            let mut case_analyzer = ReturnPathAnalyzer::new(func.return_type.unwrap());
            for stmt in &case.statements {
                self.validate_statement(
                    stmt, func, symbol_table, local_scope, result,
                    nesting_depth + 1, max_depth, &mut case_analyzer,
                );
            }
            case_analyzer.all_paths_return()
        });

        let default_returns = if let Some(default) = default_case {
            let mut analyzer = ReturnPathAnalyzer::new(func.return_type.unwrap());
            for stmt in &default.statements {
                self.validate_statement(
                    stmt, func, symbol_table, local_scope, result,
                    nesting_depth + 1, max_depth, &mut analyzer,
                );
            }
            analyzer.all_paths_return()
        } else {
            false
        };

        if all_cases_return && has_default && default_returns {
            return_path.add_return();
        }
    }

    // ==================== ASSIGNMENT VALIDATION ====================

    fn validate_assignment_statement(
        &self,
        variable: &str,
        value: &Expression,
        func: &QuickFunction,
        symbol_table: &SymbolTable,
        local_scope: &mut LocalScopeTracker,
        result: &mut SectionAnalysisResult,
    ) {
        if !Self::is_valid_identifier(variable) {
            self.add_error(
                result,
                "QFUNC017",
                "INVALID_VARIABLE_NAME",
                &format!("Invalid variable name '{}' in function '{}'", variable, func.name),
                "Variable names must start with a letter and contain only alphanumeric characters and underscores.",
                value.position(),
            );
            return;
        }

        if !local_scope.has_variable(variable) {
            self.add_error(
                result,
                "QFUNC072",
                "UNDECLARED_VARIABLE",
                &format!("Variable '{}' used before declaration in function '{}'", variable, func.name),
                &format!(
                    "Declare it first: let {} = ...; or const {} = ...;",
                    variable, variable
                ),
                value.position(),
            );
            return;
        }

        if local_scope.is_const(variable) {
            self.add_error(
                result,
                "QFUNC018",
                "CONST_REASSIGNMENT",
                &format!("Cannot reassign const variable '{}' in function '{}'", variable, func.name),
                "Use 'let mut' instead of 'const' or 'let' to allow mutation.",
                value.position(),
            );
            return;
        }

        let max_depth = Self::calculate_max_depth(100);
        self.validate_expression(value, func, symbol_table, local_scope, result, max_depth);

        let local_variable_types = local_scope.get_all_variable_types();
        let visitor = TypeInferenceVisitor::new(symbol_table, Some(local_variable_types));

        let existing_type = local_scope.get_variable_type(variable);
        let new_type = visitor.infer_type_from_expression(value);

        match (existing_type, new_type) {
            (Some(existing), Some(new_t))
                if !Self::are_types_compatible_strict(new_t, existing) =>
            {
                self.add_error(
                    result,
                    "QFUNC019",
                    "TYPE_MISMATCH_REASSIGNMENT",
                    &format!(
                        "Cannot assign {:?} to variable '{}' of type {:?}",
                        new_t, variable, existing
                    ),
                    "Variable types cannot change once assigned (unless type is 'any').",
                    value.position(),
                );
            }
            (None, Some(new_t)) => {
                local_scope.update_variable_type(variable, new_t);
                if self.debug_config.is_verbose {
                    self.error_manager.log_debug(&format!(
                        "Inferred type {:?} for variable '{}'",
                        new_t, variable
                    ));
                }
            }
            _ => {}
        }
    }

    fn validate_arithmetic_assignment_statement(
        &self,
        variable: &str,
        operator: &str,
        value: &Expression,
        func: &QuickFunction,
        symbol_table: &SymbolTable,
        local_scope: &LocalScopeTracker,
        result: &mut SectionAnalysisResult,
    ) {
        if !local_scope.has_variable(variable) {
            self.add_error(
                result,
                "QFUNC020",
                "UNDEFINED_VARIABLE",
                &format!(
                    "Variable '{}' used before assignment in function '{}'",
                    variable, func.name
                ),
                "Declare the variable before using arithmetic assignment.",
                value.position(),
            );
            return;
        }

        if local_scope.is_const(variable) {
            self.add_error(
                result,
                "QFUNC021",
                "CONST_REASSIGNMENT",
                &format!("Cannot modify const variable '{}' with '{}'", variable, operator),
                "Remove 'const' to make the variable mutable.",
                value.position(),
            );
            return;
        }

        if !is_valid_arithmetic_assign_op(operator) {
            self.add_error(
                result,
                "QFUNC022",
                "INVALID_ARITHMETIC_ASSIGN_OP",
                &format!("Invalid arithmetic assignment operator '{}'", operator),
                "Valid operators: +=, -=, *=, /=, %=, **=, &=, |=, ^=, <<=, >>=",
                value.position(),
            );
            return;
        }

        let max_depth = Self::calculate_max_depth(100);
        self.validate_expression(value, func, symbol_table, local_scope, result, max_depth);

        let visitor = TypeInferenceVisitor::new(symbol_table, None);
        if let (Some(var_t), Some(val_t)) = (
            local_scope.get_variable_type(variable),
            visitor.infer_type_from_expression(value),
        ) {
            self.validate_arithmetic_operation(
                operator, var_t, val_t, &func.name, result, value.position(),
            );
        }
    }

    fn validate_object_creation_statement(
        &self,
        variable: &str,
        object: &Value,
        func: &QuickFunction,
        symbol_table: &SymbolTable,
        local_scope: &mut LocalScopeTracker,
        result: &mut SectionAnalysisResult,
    ) {
        if !Self::is_valid_identifier(variable) {
            self.add_error(
                result,
                "QFUNC023",
                "INVALID_VARIABLE_NAME",
                &format!("Invalid variable name '{}' in function '{}'", variable, func.name),
                "Variable names must start with a letter and contain only alphanumeric characters and underscores.",
                object.position(),
            );
            return;
        }

        if local_scope.is_const(variable) {
            self.add_error(
                result,
                "QFUNC024",
                "CONST_REASSIGNMENT",
                &format!("Cannot reassign const variable '{}' in function '{}'", variable, func.name),
                "Remove 'const' to allow reassignment.",
                object.position(),
            );
            return;
        }

        self.validate_object_literal_keys(object, &func.name, result);
        self.validate_value(object, func, symbol_table, local_scope, result);

        if !local_scope.has_variable(variable) {
            local_scope.add_variable(variable.to_string(), Some(DataType::Object), false);
        }
    }

    fn check_for_unused_variables(
        &self,
        func: &QuickFunction,
        local_scope: &LocalScopeTracker,
        result: &mut SectionAnalysisResult,
    ) {
        let mut collector = VariableReferenceCollector::new(&func.parameters);
        let referenced = collector.collect_from_function(func);

        for var_name in local_scope.get_declared_variable_names() {
            if !referenced.contains(var_name) {
                self.add_warning(
                    result,
                    "QFUNC_WARN005",
                    &format!(
                        "Variable '{}' declared but never used in function '{}'",
                        var_name, func.name
                    ),
                    "QUICKFUNCS",
                    func.position,
                );
            }
        }

        if self.debug_config.is_verbose {
            let declared = local_scope.get_declared_variable_names().count();
            self.error_manager.log_debug(&format!(
                "Variable usage: {}/{} variables used",
                referenced.len(),
                declared
            ));
        }
    }

    // ==================== EXPRESSION VALIDATION ====================

    fn validate_expression(
        &self,
        expr: &Expression,
        func: &QuickFunction,
        symbol_table: &SymbolTable,
        local_scope: &LocalScopeTracker,
        result: &mut SectionAnalysisResult,
        max_depth: usize,
    ) {
        if max_depth == 0 {
            self.add_error(
                result,
                "QFUNC074",
                "EXPRESSION_DEPTH_EXCEEDED",
                &format!("Maximum expression depth exceeded in function '{}'", func.name),
                "Simplify deeply nested expressions.",
                expr.position(),
            );
            return;
        }

        match expr {
            Expression::Identifier { name, .. } => {
                self.validate_identifier(name, &func.name, local_scope, symbol_table, result, expr.position());
            }
            Expression::QualifiedIdentifier { parts, arguments, .. } => {
                self.validate_qualified_identifier(
                    parts, arguments.as_ref(), func, symbol_table, local_scope, result, max_depth,
                );
            }
            Expression::QuickFuncCall { name, arguments, .. } => {
                self.validate_quick_func_call(
                    name, arguments, func, symbol_table, local_scope, result, max_depth,
                );
            }
            Expression::ImportedFunctionCall {
                namespace_name, function_name, arguments, ..
            } => {
                self.validate_imported_function_call(
                    namespace_name, function_name, arguments,
                    func, symbol_table, local_scope, result, max_depth,
                );
            }
            Expression::InstanceMethodCall { instance, method_name, arguments, .. } => {
                self.validate_instance_method_call(
                    instance, method_name, arguments,
                    func, symbol_table, local_scope, result, max_depth,
                );
            }
            Expression::StaticMethodCall { object_name, method_name, arguments, .. } => {
                self.validate_static_method_call(
                    object_name, method_name, arguments,
                    func, symbol_table, local_scope, result, max_depth,
                );
            }
            Expression::EnumAccess { namespace_name, enum_name, value, position } => {
                self.validate_enum_access(
                    namespace_name.as_deref(), enum_name, value, &func.name, symbol_table, result, *position,
                );
            }
            Expression::ArithmeticOp { left, right, operator, .. } => {
                self.validate_arithmetic_op_expression(
                    left, right, operator, func, symbol_table, local_scope, result, max_depth,
                );
            }
            Expression::BitwiseOp { left, right, operator, .. } => {
                self.validate_bitwise_op_expression(
                    left, right, operator, func, symbol_table, local_scope, result, max_depth,
                );
            }
            Expression::ComparisonOp { left, right, operator, .. } => {
                self.validate_comparison_op_expression(
                    left, right, operator, func, symbol_table, local_scope, result, max_depth,
                );
            }
            Expression::LogicalOp { left, right, operator, .. } => {
                self.validate_logical_op_expression(
                    left, right, operator, func, symbol_table, local_scope, result, max_depth,
                );
            }
            Expression::UnaryOp { operand, operator, .. } => {
                self.validate_unary_op_expression(
                    operand, operator, func, symbol_table, local_scope, result, max_depth,
                );
            }
            Expression::Conditional { condition, true_value, false_value, .. } => {
                self.validate_conditional_expression(
                    condition, true_value, false_value,
                    func, symbol_table, local_scope, result, max_depth,
                );
            }
            Expression::PropertyAccess { object, .. } => {
                self.validate_expression(
                    object, func, symbol_table, local_scope, result, max_depth - 1,
                );
            }
            Expression::IndexAccess { object, index, .. } => {
                self.validate_expression(object, func, symbol_table, local_scope, result, max_depth - 1);
                self.validate_expression(index, func, symbol_table, local_scope, result, max_depth - 1);
            }
            Expression::Value { value, .. } => {
                self.validate_value(value, func, symbol_table, local_scope, result);
            }
            Expression::Parenthesized { expression, .. } => {
                self.validate_expression(
                    expression, func, symbol_table, local_scope, result, max_depth - 1,
                );
            }
            Expression::TypeCast { expression, .. } => {
                self.validate_expression(
                    expression, func, symbol_table, local_scope, result, max_depth - 1,
                );
            }
            _ => {}
        }
    }

    fn validate_identifier(
        &self,
        name: &str,
        func_name: &str,
        local_scope: &LocalScopeTracker,
        symbol_table: &SymbolTable,
        result: &mut SectionAnalysisResult,
        position: Position,
    ) {
        if local_scope.has_variable(name)
            || local_scope.has_parameter(name)
            || symbol_table.has_enum(name)
            || symbol_table.has_function(name)
            || symbol_table.is_builtin_static_object(name)
            || symbol_table.is_imported_namespace(name)
        {
            return;
        }

        self.add_warning(
            result,
            "QFUNC_WARN001",
            &format!(
                "Identifier '{}' not found in local scope or symbol table in function '{}'",
                name, func_name
            ),
            "QUICKFUNCS",
            position,
        );
    }

    fn validate_qualified_identifier(
        &self,
        parts: &[String],
        arguments: Option<&Vec<Expression>>,
        func: &QuickFunction,
        symbol_table: &SymbolTable,
        local_scope: &LocalScopeTracker,
        result: &mut SectionAnalysisResult,
        max_depth: usize,
    ) {
        if parts.len() < 2 {
            return;
        }

        let first = &parts[0];
        let second = &parts[1];

        // Local variable/parameter — property access or method call on a local.
        if local_scope.has_variable(first) || local_scope.has_parameter(first) {
            if let Some(args) = arguments {
                for arg in args {
                    self.validate_expression(arg, func, symbol_table, local_scope, result, max_depth - 1);
                }
            }
            return;
        }

        // Local enum access: EnumName.FIELD (2 parts, no call).
        if parts.len() == 2 && arguments.is_none() && symbol_table.has_enum(first) {
            if !symbol_table.has_enum_field(first, second) {
                if let Some(fields) = symbol_table.try_get_enum(first) {
                    let valid: Vec<&String> = fields.keys().collect();
                    self.add_error(
                        result,
                        "QFUNC052",
                        "ENUM_VALUE_NOT_FOUND",
                        &format!("Enum '{}' does not have value '{}'", first, second),
                        &format!("Valid values: {}", valid.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")),
                        Position::UNKNOWN,
                    );
                }
            }
            return;
        }

        // Namespace access.
        if symbol_table.is_imported_namespace(first) {
            self.validate_namespace_access(
                parts, arguments, func, symbol_table, local_scope, result, max_depth,
            );
            return;
        }

        // Builtin static object access.
        if has_static_object(first) {
            self.validate_static_object_access(
                parts, arguments, func, symbol_table, local_scope, result, max_depth,
            );
            return;
        }

        // DATA section variable — property access is allowed.
        if symbol_table.has_data_variable(first) {
            if let Some(args) = arguments {
                for arg in args {
                    self.validate_expression(arg, func, symbol_table, local_scope, result, max_depth - 1);
                }
            }
            return;
        }

        // Unknown — will be resolved at runtime; emit a warning only.
        self.add_warning(
            result,
            "QFUNC_WARN001",
            &format!(
                "Identifier '{}' not found in scope — will be resolved at runtime",
                first
            ),
            "QUICKFUNCS",
            Position::UNKNOWN,
        );

        if let Some(args) = arguments {
            for arg in args {
                self.validate_expression(arg, func, symbol_table, local_scope, result, max_depth - 1);
            }
        }
    }

    fn validate_namespace_access(
        &self,
        parts: &[String],
        arguments: Option<&Vec<Expression>>,
        func: &QuickFunction,
        symbol_table: &SymbolTable,
        local_scope: &LocalScopeTracker,
        result: &mut SectionAnalysisResult,
        max_depth: usize,
    ) {
        let ns = &parts[0];
        let member = &parts[1];

        if parts.len() == 2 {
            if let Some(args) = arguments {
                // Namespaced function call.
                match symbol_table.get_namespaced_function(ns, member) {
                    None => {
                        self.add_error(
                            result,
                            "QFUNC045",
                            "IMPORTED_FUNCTION_NOT_FOUND",
                            &format!("Function '{}' not found in namespace '{}'", member, ns),
                            "",
                            Position::UNKNOWN,
                        );
                        return;
                    }
                    Some(info) => {
                        let expected = info.signature.parameters.len();
                        if args.len() != expected {
                            self.add_error(
                                result,
                                "QFUNC046",
                                "PARAMETER_COUNT_MISMATCH",
                                &format!(
                                    "Function '{}.{}' expects {} parameter(s) but got {}",
                                    ns, member, expected, args.len()
                                ),
                                "",
                                Position::UNKNOWN,
                            );
                        }
                    }
                }
                for arg in args {
                    self.validate_expression(arg, func, symbol_table, local_scope, result, max_depth - 1);
                }
            } else {
                // Namespaced enum reference.
                if symbol_table.get_namespaced_enum(ns, member).is_none() {
                    self.add_error(
                        result,
                        "QFUNC055",
                        "NAMESPACE_MEMBER_NOT_FOUND",
                        &format!("Namespace '{}' does not have member '{}'", ns, member),
                        "Check the imported file for available functions and enums.",
                        Position::UNKNOWN,
                    );
                }
            }
        } else if parts.len() == 3 {
            // Imported enum access: ns.Enum.Value
            let enum_name = &parts[1];
            let enum_value = &parts[2];

            match symbol_table.get_namespaced_enum(ns, enum_name) {
                None => {
                    self.add_error(
                        result,
                        "QFUNC054",
                        "IMPORTED_ENUM_NOT_FOUND",
                        &format!("Namespace '{}' does not have enum '{}'", ns, enum_name),
                        "Check the imported file for available enums.",
                        Position::UNKNOWN,
                    );
                }
                Some(fields) if !fields.contains_key(enum_value) => {
                    let valid: Vec<&String> = fields.keys().collect();
                    self.add_error(
                        result,
                        "QFUNC056",
                        "ENUM_VALUE_NOT_FOUND",
                        &format!("Enum '{}.{}' does not have value '{}'", ns, enum_name, enum_value),
                        &format!("Valid values: {}", valid.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")),
                        Position::UNKNOWN,
                    );
                }
                _ => {}
            }
        }
    }

    fn validate_static_object_access(
        &self,
        parts: &[String],
        arguments: Option<&Vec<Expression>>,
        func: &QuickFunction,
        symbol_table: &SymbolTable,
        local_scope: &LocalScopeTracker,
        result: &mut SectionAnalysisResult,
        max_depth: usize,
    ) {
        let object_name = &parts[0];
        let method_name = &parts[1];

        if let Some(args) = arguments {
            if !has_static_method(object_name, method_name) {
                self.add_error(
                    result,
                    "QFUNC050",
                    "STATIC_METHOD_NOT_FOUND",
                    &format!("Static object '{}' has no method '{}'", object_name, method_name),
                    "",
                    Position::UNKNOWN,
                );
            }
            for arg in args {
                self.validate_expression(arg, func, symbol_table, local_scope, result, max_depth - 1);
            }
        }
    }

    // ==================== OPERATOR EXPRESSION VALIDATORS ====================

    fn validate_arithmetic_op_expression(
        &self,
        left: &Expression,
        right: &Expression,
        operator: &str,
        func: &QuickFunction,
        symbol_table: &SymbolTable,
        local_scope: &LocalScopeTracker,
        result: &mut SectionAnalysisResult,
        max_depth: usize,
    ) {
        if !is_valid_arithmetic_operator(operator) {
            self.add_error(
                result,
                "QFUNC025",
                "INVALID_ARITHMETIC_OPERATOR",
                &format!("Invalid arithmetic operator '{}' in function '{}'", operator, func.name),
                "Valid operators: +, -, *, /, %, **, %%, %&, &%",
                left.position(),
            );
            return;
        }

        self.validate_expression(left, func, symbol_table, local_scope, result, max_depth - 1);
        self.validate_expression(right, func, symbol_table, local_scope, result, max_depth - 1);

        let visitor = TypeInferenceVisitor::new(symbol_table, None);
        let lt = visitor.infer_type_from_expression(left);
        let rt = visitor.infer_type_from_expression(right);

        if let (Some(l), Some(r)) = (lt, rt) {
            if operator == "+" {
                match (l, r) {
                    (DataType::String, DataType::String) => return,
                    (DataType::String, _) | (_, DataType::String) => {
                        self.add_error(
                            result,
                            "QFUNC026",
                            "INVALID_STRING_OPERATION",
                            &format!(
                                "Cannot concatenate string with {:?} in function '{}'",
                                if l == DataType::String { r } else { l },
                                func.name
                            ),
                            "Use only string + string, or convert to string first.",
                            left.position(),
                        );
                        return;
                    }
                    _ => {}
                }
            }

            if !is_numeric_type(l) {
                self.add_error(
                    result, "QFUNC027", "NON_NUMERIC_OPERAND",
                    &format!("Left operand of '{}' must be numeric, got {:?} in '{}'", operator, l, func.name),
                    "Use int, float, or double.", left.position(),
                );
            }
            if !is_numeric_type(r) {
                self.add_error(
                    result, "QFUNC028", "NON_NUMERIC_OPERAND",
                    &format!("Right operand of '{}' must be numeric, got {:?} in '{}'", operator, r, func.name),
                    "Use int, float, or double.", right.position(),
                );
            }
        }
    }

    fn validate_bitwise_op_expression(
        &self,
        left: &Expression,
        right: &Expression,
        operator: &str,
        func: &QuickFunction,
        symbol_table: &SymbolTable,
        local_scope: &LocalScopeTracker,
        result: &mut SectionAnalysisResult,
        max_depth: usize,
    ) {
        if !is_valid_bitwise_operator(operator) {
            self.add_error(
                result, "QFUNC029", "INVALID_BITWISE_OPERATOR",
                &format!("Invalid bitwise operator '{}' in function '{}'", operator, func.name),
                "Valid operators: &, |, ^, <<, >>", left.position(),
            );
            return;
        }

        self.validate_expression(left, func, symbol_table, local_scope, result, max_depth - 1);
        self.validate_expression(right, func, symbol_table, local_scope, result, max_depth - 1);

        let visitor = TypeInferenceVisitor::new(symbol_table, None);
        for (side, expr) in [("Left", left), ("Right", right)] {
            if let Some(t) = visitor.infer_type_from_expression(expr) {
                if t != DataType::Int {
                    let code = if side == "Left" { "QFUNC030" } else { "QFUNC031" };
                    self.add_error(
                        result, code, "NON_INT_BITWISE_OPERAND",
                        &format!("{} operand of '{}' must be int, got {:?} in '{}'", side, operator, t, func.name),
                        "Convert to int or use arithmetic operators instead.", expr.position(),
                    );
                }
            }
        }
    }

    fn validate_comparison_op_expression(
        &self,
        left: &Expression,
        right: &Expression,
        operator: &str,
        func: &QuickFunction,
        symbol_table: &SymbolTable,
        local_scope: &LocalScopeTracker,
        result: &mut SectionAnalysisResult,
        max_depth: usize,
    ) {
        if !is_valid_comparison_operator(operator) {
            self.add_error(
                result, "QFUNC032", "INVALID_COMPARISON_OPERATOR",
                &format!("Invalid comparison operator '{}' in function '{}'", operator, func.name),
                "Valid operators: ==, !=, >, <, >=, <=", left.position(),
            );
            return;
        }

        self.validate_expression(left, func, symbol_table, local_scope, result, max_depth - 1);
        self.validate_expression(right, func, symbol_table, local_scope, result, max_depth - 1);

        let visitor = TypeInferenceVisitor::new(symbol_table, None);
        let lt = visitor.infer_type_from_expression(left);
        let rt = visitor.infer_type_from_expression(right);

        if let (Some(l), Some(r)) = (lt, rt) {
            if operator == "==" || operator == "!=" {
                if !Self::are_types_comparable(l, r) {
                    self.add_warning(
                        result, "QFUNC_WARN002",
                        &format!("Comparing incompatible types {:?} and {:?} in function '{}'", l, r, func.name),
                        "QUICKFUNCS", left.position(),
                    );
                }
                return;
            }
            if !is_numeric_type(l) || !is_numeric_type(r) {
                self.add_error(
                    result, "QFUNC033", "NON_NUMERIC_COMPARISON",
                    &format!("Operator '{}' requires numeric types, got {:?} and {:?} in '{}'", operator, l, r, func.name),
                    "Use numeric types (int, float, double) for relational comparisons.", left.position(),
                );
            }
        }
    }

    fn validate_logical_op_expression(
        &self,
        left: &Expression,
        right: &Expression,
        operator: &str,
        func: &QuickFunction,
        symbol_table: &SymbolTable,
        local_scope: &LocalScopeTracker,
        result: &mut SectionAnalysisResult,
        max_depth: usize,
    ) {
        if !is_valid_logical_operator(operator) {
            self.add_error(
                result, "QFUNC034", "INVALID_LOGICAL_OPERATOR",
                &format!("Invalid logical operator '{}' in function '{}'", operator, func.name),
                "Valid operators: &&, ||, and, or", left.position(),
            );
            return;
        }

        self.validate_expression(left, func, symbol_table, local_scope, result, max_depth - 1);
        self.validate_expression(right, func, symbol_table, local_scope, result, max_depth - 1);

        let visitor = TypeInferenceVisitor::new(symbol_table, None);
        for (code, expr, side) in [
            ("QFUNC035", left, "Left"),
            ("QFUNC036", right, "Right"),
        ] {
            if let Some(t) = visitor.infer_type_from_expression(expr) {
                if t != DataType::Bool {
                    self.add_error(
                        result, code, "NON_BOOL_LOGICAL_OPERAND",
                        &format!("{} operand of '{}' must be bool, got {:?} in '{}'", side, operator, t, func.name),
                        "Use comparison operators to create boolean values.", expr.position(),
                    );
                }
            }
        }
    }

    fn validate_unary_op_expression(
        &self,
        operand: &Expression,
        operator: &str,
        func: &QuickFunction,
        symbol_table: &SymbolTable,
        local_scope: &LocalScopeTracker,
        result: &mut SectionAnalysisResult,
        max_depth: usize,
    ) {
        if !is_valid_unary_operator(operator) {
            self.add_error(
                result, "QFUNC037", "INVALID_UNARY_OPERATOR",
                &format!("Invalid unary operator '{}' in function '{}'", operator, func.name),
                "Valid operators: !, not, -, +, ~?", operand.position(),
            );
            return;
        }

        self.validate_expression(operand, func, symbol_table, local_scope, result, max_depth - 1);

        let visitor = TypeInferenceVisitor::new(symbol_table, None);
        if let Some(ot) = visitor.infer_type_from_expression(operand) {
            match operator {
                "!" | "not" if ot != DataType::Bool => {
                    self.add_error(
                        result, "QFUNC038", "NON_BOOL_NOT_OPERAND",
                        &format!("Logical NOT requires bool, got {:?} in '{}'", ot, func.name),
                        "Use a comparison to create a boolean value.", operand.position(),
                    );
                }
                "~?" if ot != DataType::Int => {
                    self.add_error(
                        result, "QFUNC039", "NON_INT_BITWISE_NOT",
                        &format!("Bitwise NOT (~?) requires int, got {:?} in '{}'", ot, func.name),
                        "Convert to int before using bitwise NOT.", operand.position(),
                    );
                }
                "-" | "+" if !is_numeric_type(ot) => {
                    self.add_error(
                        result, "QFUNC040", "NON_NUMERIC_UNARY",
                        &format!("Unary '{}' requires numeric type, got {:?} in '{}'", operator, ot, func.name),
                        "Use int, float, or double.", operand.position(),
                    );
                }
                _ => {}
            }
        }
    }

    fn validate_conditional_expression(
        &self,
        condition: &Expression,
        true_value: &Expression,
        false_value: &Expression,
        func: &QuickFunction,
        symbol_table: &SymbolTable,
        local_scope: &LocalScopeTracker,
        result: &mut SectionAnalysisResult,
        max_depth: usize,
    ) {
        self.validate_expression(condition, func, symbol_table, local_scope, result, max_depth - 1);

        let visitor = TypeInferenceVisitor::new(symbol_table, None);
        if let Some(ct) = visitor.infer_type_from_expression(condition) {
            if ct != DataType::Bool {
                self.add_error(
                    result, "QFUNC041", "NON_BOOL_TERNARY_CONDITION",
                    &format!("Ternary condition must be bool, got {:?} in '{}'", ct, func.name),
                    "Use comparison operators to create a boolean condition.", condition.position(),
                );
            }
        }

        self.validate_expression(true_value, func, symbol_table, local_scope, result, max_depth - 1);
        self.validate_expression(false_value, func, symbol_table, local_scope, result, max_depth - 1);

        let tt = visitor.infer_type_from_expression(true_value);
        let ft = visitor.infer_type_from_expression(false_value);

        if let (Some(t), Some(f)) = (tt, ft) {
            if !Self::are_types_comparable(t, f) {
                self.add_warning(
                    result, "QFUNC_WARN003",
                    &format!("Ternary branches have incompatible types {:?} and {:?} in '{}'", t, f, func.name),
                    "QUICKFUNCS", condition.position(),
                );
            }
        }
    }

    // ==================== CALL VALIDATORS ====================

    fn validate_quick_func_call(
        &self,
        name: &str,
        arguments: &[Expression],
        func: &QuickFunction,
        symbol_table: &SymbolTable,
        local_scope: &LocalScopeTracker,
        result: &mut SectionAnalysisResult,
        max_depth: usize,
    ) {
        // Lambda invocation via local variable — pass through.
        if local_scope.has_variable(name) {
            for arg in arguments {
                self.validate_expression(arg, func, symbol_table, local_scope, result, max_depth - 1);
            }
            return;
        }

        if !symbol_table.has_function(name) {
            self.add_error(
                result, "QFUNC042", "FUNCTION_NOT_FOUND",
                &format!("Function '{}' is not defined in @QUICKFUNCS", name),
                "Define the function in @QUICKFUNCS or check the spelling.",
                Position::UNKNOWN,
            );
            return;
        }

        if let Some(sig) = symbol_table.try_get_function(name) {
            let expected = sig.parameters.len();
            if arguments.len() != expected {
                self.add_error(
                    result, "QFUNC043", "WRONG_ARGUMENT_COUNT",
                    &format!(
                        "Function '{}' expects {} arguments, got {}",
                        name, expected, arguments.len()
                    ),
                    &format!("Check the function signature: {}", sig),
                    Position::UNKNOWN,
                );
            }
        }

        for arg in arguments {
            self.validate_expression(arg, func, symbol_table, local_scope, result, max_depth - 1);
        }
    }

    fn validate_imported_function_call(
        &self,
        namespace_name: &str,
        function_name: &str,
        arguments: &[Expression],
        func: &QuickFunction,
        symbol_table: &SymbolTable,
        local_scope: &LocalScopeTracker,
        result: &mut SectionAnalysisResult,
        max_depth: usize,
    ) {
        // Treat as instance method call on a local variable.
        if local_scope.has_variable(namespace_name) {
            for arg in arguments {
                self.validate_expression(arg, func, symbol_table, local_scope, result, max_depth - 1);
            }
            return;
        }

        if !symbol_table.is_imported_namespace(namespace_name) {
            self.add_error(
                result, "QFUNC044", "NAMESPACE_NOT_FOUND",
                &format!(
                    "Namespace '{}' not found. Did you import it in @IMPORTS?",
                    namespace_name
                ),
                &format!(
                    "Add to @IMPORTS: {} from \"path/to/file.mdix\"",
                    namespace_name
                ),
                Position::UNKNOWN,
            );
            return;
        }

        match symbol_table.get_namespaced_function(namespace_name, function_name) {
            None => {
                let suggestion = symbol_table
                    .try_get_namespace(namespace_name)
                    .map(|ns| {
                        let names: Vec<&String> = ns.functions.keys().collect();
                        format!(
                            "Available functions: {}",
                            names.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
                        )
                    })
                    .unwrap_or_default();

                self.add_error(
                    result, "QFUNC045", "IMPORTED_FUNCTION_NOT_FOUND",
                    &format!(
                        "Function '{}' not found in namespace '{}'",
                        function_name, namespace_name
                    ),
                    &suggestion,
                    Position::UNKNOWN,
                );
            }
            Some(info) => {
                let expected = info.signature.parameters.len();
                if arguments.len() != expected {
                    self.add_error(
                        result, "QFUNC046", "PARAMETER_COUNT_MISMATCH",
                        &format!(
                            "Function '{}.{}' expects {} parameter(s) but got {}",
                            namespace_name, function_name, expected, arguments.len()
                        ),
                        &format!("Expected: {}", info.signature),
                        Position::UNKNOWN,
                    );
                }
                if self.debug_config.is_verbose {
                    self.error_manager.log_debug(&format!(
                        "Validated {}.{}() — return type {:?}",
                        namespace_name, function_name, info.signature.return_type
                    ));
                }
            }
        }

        for arg in arguments {
            self.validate_expression(arg, func, symbol_table, local_scope, result, max_depth - 1);
        }
    }

    fn validate_instance_method_call(
        &self,
        instance: &Expression,
        method_name: &str,
        arguments: &[Expression],
        func: &QuickFunction,
        symbol_table: &SymbolTable,
        local_scope: &LocalScopeTracker,
        result: &mut SectionAnalysisResult,
        max_depth: usize,
    ) {
        let chain_depth = Self::count_method_chain_depth(instance);
        if chain_depth > MAX_METHOD_CHAIN_DEPTH {
            self.add_error(
                result, "QFUNC066", "METHOD_CHAIN_TOO_DEEP",
                &format!(
                    "Method chain depth ({}) exceeds maximum of {} in function '{}'",
                    chain_depth, MAX_METHOD_CHAIN_DEPTH, func.name
                ),
                "Break up the method chain into intermediate variables.", instance.position(),
            );
            return;
        }

        self.validate_expression(instance, func, symbol_table, local_scope, result, max_depth - 1);

        let visitor = TypeInferenceVisitor::new(symbol_table, None);
        if let Some(inst_type) = visitor.infer_type_from_expression(instance) {
            if let Some(dix_type) = Self::convert_data_type_to_dix_type(inst_type) {
                if !has_instance_method(dix_type, method_name) {
                    self.add_error(
                        result, "QFUNC047", "INSTANCE_METHOD_NOT_FOUND",
                        &format!("Type '{:?}' has no instance method '{}'", inst_type, method_name),
                        &format!("Type '{:?}' has no such method.", inst_type),
                        instance.position(),
                    );
                }
            }
        }

        for arg in arguments {
            self.validate_expression(arg, func, symbol_table, local_scope, result, max_depth - 1);
        }
    }

    fn validate_static_method_call(
        &self,
        object_name: &str,
        method_name: &str,
        arguments: &[Expression],
        func: &QuickFunction,
        symbol_table: &SymbolTable,
        local_scope: &LocalScopeTracker,
        result: &mut SectionAnalysisResult,
        max_depth: usize,
    ) {
        if !has_static_object(object_name) {
            self.add_error(
                result, "QFUNC049", "STATIC_OBJECT_NOT_FOUND",
                &format!("Static object '{}' is not defined", object_name),
                "Available static objects: Math, DateTime, Array, Random, Enum, Guid, IpAddress, Dix",
                Position::UNKNOWN,
            );
        } else if !has_static_method(object_name, method_name) {
            self.add_error(
                result, "QFUNC050", "STATIC_METHOD_NOT_FOUND",
                &format!("Static object '{}' has no method '{}'", object_name, method_name),
                "",
                Position::UNKNOWN,
            );
        }

        for arg in arguments {
            self.validate_expression(arg, func, symbol_table, local_scope, result, max_depth - 1);
        }
    }

    fn validate_enum_access(
        &self,
        namespace_name: Option<&str>,
        enum_name: &str,
        value: &str,
        function_name: &str,
        symbol_table: &SymbolTable,
        result: &mut SectionAnalysisResult,
        position: Position,
    ) {
        if self.debug_config.is_verbose {
            let fqn = match namespace_name {
                Some(ns) => format!("{}.{}.{}", ns, enum_name, value),
                None => format!("{}.{}", enum_name, value),
            };
            self.error_manager.log_debug(&format!("Validating enum access: {}", fqn));
        }

        match namespace_name {
            Some(ns) => {
                match symbol_table.get_namespaced_enum(ns, enum_name) {
                    None => {
                        let suggestion = symbol_table
                            .try_get_namespace(ns)
                            .map(|n| {
                                let names: Vec<&String> = n.enums.keys().collect();
                                if names.is_empty() {
                                    String::new()
                                } else {
                                    format!("Available enums: {}", names.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", "))
                                }
                            })
                            .unwrap_or_default();

                        self.add_error(
                            result, "QFUNC055", "IMPORTED_ENUM_NOT_FOUND",
                            &format!("Enum '{}' not found in namespace '{}'", enum_name, ns),
                            &suggestion, position,
                        );
                    }
                    Some(fields) if !fields.contains_key(value) => {
                        let valid: Vec<&String> = fields.keys().collect();
                        self.add_error(
                            result, "QFUNC056", "ENUM_VALUE_NOT_FOUND",
                            &format!("Enum '{}.{}' does not have value '{}'", ns, enum_name, value),
                            &format!("Valid values: {}", valid.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")),
                            position,
                        );
                    }
                    _ => {}
                }
            }
            None => {
                if !symbol_table.has_enum(enum_name) {
                    self.add_error(
                        result, "QFUNC052", "ENUM_NOT_FOUND",
                        &format!("Enum '{}' not defined in @ENUMS section", enum_name),
                        "Define the enum in @ENUMS or check the spelling.", position,
                    );
                    return;
                }
                if !symbol_table.has_enum_field(enum_name, value) {
                    if let Some(fields) = symbol_table.try_get_enum(enum_name) {
                        let valid: Vec<&String> = fields.keys().collect();
                        self.add_error(
                            result, "QFUNC053", "ENUM_VALUE_NOT_FOUND",
                            &format!("Enum '{}' does not have value '{}'", enum_name, value),
                            &format!("Valid values: {}", valid.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")),
                            position,
                        );
                    }
                }
            }
        }
    }

    // ==================== VALUE VALIDATION ====================

    fn validate_value(
        &self,
        value: &Value,
        func: &QuickFunction,
        symbol_table: &SymbolTable,
        local_scope: &LocalScopeTracker,
        result: &mut SectionAnalysisResult,
    ) {
        match value {
            Value::Array { values, .. } => {
                if values.len() > MAX_ARRAY_ELEMENTS {
                    self.add_error(
                        result, "QFUNC057", "ARRAY_TOO_LARGE",
                        &format!("Array has {} elements, exceeds limit of {}", values.len(), MAX_ARRAY_ELEMENTS),
                        &format!("Reduce array size to {} or fewer elements", MAX_ARRAY_ELEMENTS),
                        value.position(),
                    );
                }
                self.validate_array_homogeneity(values, &func.name, local_scope, symbol_table, result, value.position());
                for item in values {
                    self.validate_value(item, func, symbol_table, local_scope, result);
                }
            }
            Value::Object { properties, .. } => {
                self.validate_object_literal_keys(value, &func.name, result);
                if properties.len() > MAX_OBJECT_PROPERTIES {
                    self.add_error(
                        result, "QFUNC058", "OBJECT_TOO_LARGE",
                        &format!("Object has {} properties, exceeds limit of {}", properties.len(), MAX_OBJECT_PROPERTIES),
                        &format!("Reduce to {} or fewer properties", MAX_OBJECT_PROPERTIES),
                        value.position(),
                    );
                }
                for prop in properties {
                    self.validate_value(&prop.value, func, symbol_table, local_scope, result);
                }
            }
            Value::PrefixedConstructor { prefix, arguments, .. } => {
                if prefix.eq_ignore_ascii_case("t") && arguments.len() > MAX_TUPLE_ARGUMENTS {
                    self.add_error(
                        result, "QFUNC059", "TUPLE_TOO_LARGE",
                        &format!("Tuple has {} arguments, exceeds limit of {}", arguments.len(), MAX_TUPLE_ARGUMENTS),
                        &format!("Reduce tuple size to {} or fewer arguments", MAX_TUPLE_ARGUMENTS),
                        value.position(),
                    );
                }
                for arg in arguments {
                    self.validate_value(arg, func, symbol_table, local_scope, result);
                }
            }
            Value::InterpolatedString { expressions, .. } => {
                let max_depth = Self::calculate_max_depth(100);
                for expr in expressions {
                    self.validate_expression(expr, func, symbol_table, local_scope, result, max_depth);
                }
            }
            Value::Expression { expr, .. } => {
                let max_depth = Self::calculate_max_depth(100);
                self.validate_expression(expr, func, symbol_table, local_scope, result, max_depth);
            }
            _ => {}
        }
    }

    fn validate_array_homogeneity(
        &self,
        values: &[Value],
        function_name: &str,
        local_scope: &LocalScopeTracker,
        symbol_table: &SymbolTable,
        result: &mut SectionAnalysisResult,
        position: Position,
    ) {
        if values.len() < 2 {
            return;
        }

        let local_types = local_scope.get_all_variable_types();
        let visitor = TypeInferenceVisitor::new(symbol_table, Some(local_types));

        let first_type = match visitor.infer_type_from_value(&values[0]) {
            Some(t) => t,
            None => return,
        };

        for (i, element) in values.iter().enumerate().skip(1) {
            match visitor.infer_type_from_value(element) {
                Some(et) if !Self::are_types_compatible_strict(et, first_type) => {
                    self.add_error(
                        result, "QFUNC077", "ARRAY_HETEROGENEOUS",
                        &format!(
                            "Array element {} has type {:?} but array expects {:?} (from first element)",
                            i + 1, et, first_type
                        ),
                        &format!(
                            "All array elements must be the same type. Convert element to {:?} or use separate arrays.",
                            first_type
                        ),
                        position,
                    );
                }
                None => {
                    self.add_warning(
                        result, "QFUNC_WARN008",
                        &format!(
                            "Cannot infer type of array element {} in function '{}'",
                            i + 1, function_name
                        ),
                        "QUICKFUNCS", position,
                    );
                }
                _ => {}
            }
        }

        if self.debug_config.is_verbose {
            self.error_manager.log_debug(&format!(
                "Array homogeneity validated: all {} elements are {:?}",
                values.len(), first_type
            ));
        }
    }

    fn validate_object_literal_keys(
        &self,
        object: &Value,
        function_name: &str,
        result: &mut SectionAnalysisResult,
    ) {
        let properties = match object {
            Value::Object { properties, .. } => properties,
            _ => return,
        };

        let mut seen: FxHashSet<&str> =
            FxHashSet::with_capacity_and_hasher(properties.len(), Default::default());

        for prop in properties {
            if !seen.insert(prop.key.as_str()) {
                self.add_error(
                    result, "QFUNC060", "DUPLICATE_OBJECT_KEY",
                    &format!("Duplicate object key '{}' in function '{}'", prop.key, function_name),
                    &format!("Each key must be unique. Remove or rename duplicate key '{}'.", prop.key),
                    prop.position,
                );
            }
        }
    }

    // ==================== TYPE SYSTEM ====================

    #[inline]
    fn are_types_compatible_strict(source: DataType, target: DataType) -> bool {
        source == target
            || source == DataType::Any
            || target == DataType::Any
            || (is_numeric_type(source) && is_numeric_type(target))
            || matches!(
                (source, target),
                (DataType::Date, DataType::Timestamp) | (DataType::Timestamp, DataType::Date)
            )
    }

    #[inline]
    fn are_types_comparable(a: DataType, b: DataType) -> bool {
        a == b
            || a == DataType::Any
            || b == DataType::Any
            || (is_numeric_type(a) && is_numeric_type(b))
            || matches!(
                (a, b),
                (DataType::Date, DataType::Timestamp)
                    | (DataType::Timestamp, DataType::Date)
                    | (DataType::Timestamp, DataType::Timestamp)
                    | (DataType::Date, DataType::Date)
            )
    }

    fn validate_arithmetic_operation(
        &self,
        op: &str,
        left_type: DataType,
        right_type: DataType,
        function_name: &str,
        result: &mut SectionAnalysisResult,
        position: Position,
    ) {
        if op == "+=" {
            match (left_type, right_type) {
                (DataType::String, DataType::String) => return,
                (DataType::String, _) | (_, DataType::String) => {
                    self.add_error(
                        result, "QFUNC061", "INVALID_STRING_CONCAT_ASSIGN",
                        &format!("Cannot use '+=' to concatenate string with non-string in function '{}'", function_name),
                        "Use only string += string, or convert to string first.", position,
                    );
                    return;
                }
                _ => {}
            }
        }

        if !is_numeric_type(left_type) {
            self.add_error(
                result, "QFUNC062", "NON_NUMERIC_ARITHMETIC_ASSIGN",
                &format!("Arithmetic assignment '{}' requires numeric type, left operand is {:?}", op, left_type),
                "Use int, float, or double.", position,
            );
        }
        if !is_numeric_type(right_type) {
            self.add_error(
                result, "QFUNC063", "NON_NUMERIC_ARITHMETIC_ASSIGN",
                &format!("Arithmetic assignment '{}' requires numeric type, right operand is {:?}", op, right_type),
                "Use int, float, or double.", position,
            );
        }

        if matches!(op, "&=" | "|=" | "^=" | "<<=" | ">>=") {
            if left_type != DataType::Int {
                self.add_error(
                    result, "QFUNC064", "NON_INT_BITWISE_ASSIGN",
                    &format!("Bitwise assignment '{}' requires int, got {:?}", op, left_type),
                    "Convert to int before using bitwise assignment.", position,
                );
            }
            if right_type != DataType::Int {
                self.add_error(
                    result, "QFUNC065", "NON_INT_BITWISE_ASSIGN",
                    &format!("Bitwise assignment '{}' requires int, got {:?}", op, right_type),
                    "Convert to int before using bitwise assignment.", position,
                );
            }
        }
    }

    #[inline]
    fn convert_data_type_to_dix_type(data_type: DataType) -> Option<DixType> {
        match data_type {
            DataType::Int       => Some(DixType::Int),
            DataType::Float     => Some(DixType::Float),
            DataType::Double    => Some(DixType::Double),
            DataType::String    => Some(DixType::String),
            DataType::Bool      => Some(DixType::Bool),
            DataType::Array     => Some(DixType::Array),
            DataType::Tuple     => Some(DixType::Tuple),
            DataType::Object    => Some(DixType::Object),
            DataType::Hex       => Some(DixType::Hex),
            DataType::Blob      => Some(DixType::Blob),
            DataType::Regex     => Some(DixType::Regex),
            DataType::Date      => Some(DixType::Date),
            DataType::Timestamp => Some(DixType::Timestamp),
            DataType::Enum      => Some(DixType::Enum),
            _ => None,
        }
    }

    /// Count how many chained method/property accesses precede the root.
    /// Result is cached implicitly because we only call this at the call site.
    fn count_method_chain_depth(expr: &Expression) -> usize {
        let mut depth = 0;
        let mut current = expr;
        loop {
            match current {
                Expression::InstanceMethodCall { instance, .. } => {
                    depth += 1;
                    current = instance;
                }
                Expression::PropertyAccess { object, .. } => {
                    depth += 1;
                    current = object;
                }
                _ => break,
            }
        }
        depth
    }

    // ==================== SYMBOL TABLE POPULATION ====================

    fn populate_symbol_table(
        &self,
        section: &QuickFuncsSection,
        symbol_table: &mut SymbolTable,
        duplicate_functions: &FxHashSet<&str>,
        _result: &mut SectionAnalysisResult,
    ) {
        let mut registered = 0usize;

        for func in &section.functions {
            if duplicate_functions.contains(func.name.as_str()) {
                continue;
            }

            let parameters: Vec<ParameterInfo> = func
                .parameters
                .iter()
                .filter(|p| Self::is_valid_identifier(&p.name))
                .map(|p| ParameterInfo {
                    name: p.name.clone(),
                    param_type: p.data_type,
                    has_default_value: p.default_value.is_some(),
                    default_value: p.default_value.clone(),
                })
                .collect();

            let scopes = func.scope_list.clone().unwrap_or_default();

            let signature = FunctionSignature {
                name: func.name.clone(),
                return_type: func.return_type,
                parameters,
                scopes,
                line: func.position.line as i32,
                column: func.position.column as i32,
            };

            symbol_table.add_function(func.name.clone(), signature);
            registered += 1;

            if self.debug_config.is_verbose {
                self.error_manager.log_debug(&format!(
                    "Pre-registered function '{}' in symbol table",
                    func.name
                ));
            }
        }

        if self.debug_config.is_enabled {
            self.error_manager.log_info(&format!(
                "Symbol table populated: {} functions registered, {} duplicates skipped",
                registered,
                duplicate_functions.len()
            ));
        }
    }

    // ==================== HELPERS ====================

    #[inline]
    fn is_valid_identifier(name: &str) -> bool {
        let mut chars = name.chars();
        match chars.next() {
            Some(c) if c.is_ascii_alphabetic() || c == '_' => {
                chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
            }
            _ => false,
        }
    }

    #[inline]
    fn is_valid_data_path(path: &str) -> bool {
        !path.is_empty()
            && path.split('.').all(|seg| !seg.is_empty() && Self::is_valid_identifier(seg))
    }

    #[inline]
    fn should_halt(&self, result: &SectionAnalysisResult) -> bool {
        !result.errors.is_empty()
            && self.operational_settings.error_handling_strategy == ErrorHandlingStrategy::Halt
    }

    #[inline]
    fn add_error(
        &self,
        result: &mut SectionAnalysisResult,
        error_id: &str,
        error_type: &str,
        message: &str,
        suggestion: &str,
        position: Position,
    ) {
        result.errors.push(SemanticErrorInfo {
            error_id:    error_id.to_string(),
            error_type:  error_type.to_string(),
            message:     message.to_string(),
            section_name: "QUICKFUNCS".to_string(),
            suggestion:  suggestion.to_string(),
            position:    Some(position),
        });
        if self.debug_config.is_enabled {
            self.error_manager.log_error(&format!("[{}] {}: {}", error_id, error_type, message));
        }
    }

    #[inline]
    fn add_warning(
        &self,
        result: &mut SectionAnalysisResult,
        warning_id: &str,
        message: &str,
        section_name: &str,
        position: Position,
    ) {
        result.warnings.push(SemanticWarningInfo {
            warning_id:   warning_id.to_string(),
            message:      message.to_string(),
            section_name: section_name.to_string(),
            position:     Some(position),
        });
        if self.debug_config.is_enabled {
            self.error_manager.log_debug(&format!("[{}] {}", warning_id, message));
        }
    }
}

// ==================== LOCAL SCOPE TRACKER ====================

/// Tracks local variables and parameters within a single function body.
/// Allocated once before the function loop and reused via `reset_with_params`.
struct LocalScopeTracker {
    variables: FxHashMap<String, VariableScopeInfo>,
    parameters: FxHashSet<String>,
}

impl LocalScopeTracker {
    fn with_capacity(capacity: usize) -> Self {
        LocalScopeTracker {
            variables: FxHashMap::with_capacity_and_hasher(capacity, Default::default()),
            parameters: FxHashSet::with_capacity_and_hasher(capacity, Default::default()),
        }
    }

    /// Clear and reinitialise for a new function, avoiding heap reallocation.
    fn reset_with_params(&mut self, func_parameters: &[QuickFuncParam]) {
        self.variables.clear();
        self.parameters.clear();

        for param in func_parameters {
            self.parameters.insert(param.name.clone());
            self.variables.insert(
                param.name.clone(),
                VariableScopeInfo {
                    var_type:     param.data_type,
                    is_const:     true,
                    is_parameter: true,
                },
            );
        }
    }

    #[inline]
    fn add_variable(&mut self, name: String, var_type: Option<DataType>, is_const: bool) {
        self.variables.insert(name, VariableScopeInfo { var_type, is_const, is_parameter: false });
    }

    #[inline]
    fn has_variable(&self, name: &str) -> bool {
        self.variables.contains_key(name)
    }

    #[inline]
    fn has_parameter(&self, name: &str) -> bool {
        self.parameters.contains(name)
    }

    #[inline]
    fn is_const(&self, name: &str) -> bool {
        self.variables.get(name).map_or(false, |v| v.is_const)
    }

    #[inline]
    fn get_variable_type(&self, name: &str) -> Option<DataType> {
        self.variables.get(name).and_then(|v| v.var_type)
    }

    fn update_variable_type(&mut self, name: &str, var_type: DataType) {
        if let Some(info) = self.variables.get_mut(name) {
            if info.var_type.is_none() {
                info.var_type = Some(var_type);
            }
        }
    }

    fn get_declared_variable_names(&self) -> impl Iterator<Item = &String> {
        self.variables
            .iter()
            .filter(|(_, v)| !v.is_parameter)
            .map(|(k, _)| k)
    }

    fn get_all_variable_types(&self) -> HashMap<String, Option<DataType>> {
        self.variables
            .iter()
            .map(|(k, v)| (k.clone(), v.var_type))
            .collect()
    }
}

struct VariableScopeInfo {
    var_type:     Option<DataType>,
    is_const:     bool,
    is_parameter: bool,
}

// ==================== RETURN PATH ANALYZER ====================

struct ReturnPathAnalyzer {
    _expected_return_type: DataType,
    has_unconditional_return: bool,
}

impl ReturnPathAnalyzer {
    #[inline]
    fn new(expected_return_type: DataType) -> Self {
        ReturnPathAnalyzer {
            _expected_return_type: expected_return_type,
            has_unconditional_return: false,
        }
    }

    #[inline]
    fn add_return(&mut self) {
        self.has_unconditional_return = true;
    }

    #[inline]
    fn all_paths_return(&self) -> bool {
        self.has_unconditional_return
    }
}

// ==================== VARIABLE REFERENCE COLLECTOR ====================

struct VariableReferenceCollector {
    referenced: FxHashSet<String>,
    parameters: FxHashSet<String>,
}

impl VariableReferenceCollector {
    fn new(func_parameters: &[QuickFuncParam]) -> Self {
        let parameters: FxHashSet<String> =
            func_parameters.iter().map(|p| p.name.clone()).collect();
        VariableReferenceCollector {
            referenced: FxHashSet::default(),
            parameters,
        }
    }

    fn collect_from_function(&mut self, func: &QuickFunction) -> &FxHashSet<String> {
        for stmt in &func.body {
            self.collect_from_statement(stmt);
        }
        &self.referenced
    }

    fn collect_from_statement(&mut self, stmt: &QuickFuncStatement) {
        match stmt {
            QuickFuncStatement::Return { value, .. } => self.collect_expr(value),
            QuickFuncStatement::Assignment { value, .. } => self.collect_expr(value),
            QuickFuncStatement::ArithmeticAssignment { variable, value, .. } => {
                self.add_ref(variable);
                self.collect_expr(value);
            }
            QuickFuncStatement::VariableDeclaration { value, .. } => self.collect_expr(value),
            QuickFuncStatement::ObjectCreation { object, .. } => self.collect_value(object),
            QuickFuncStatement::If { condition, then_branch, else_branch, .. } => {
                self.collect_expr(condition);
                for s in then_branch { self.collect_from_statement(s); }
                if let Some(els) = else_branch {
                    for s in els { self.collect_from_statement(s); }
                }
            }
            QuickFuncStatement::Switch { expression, cases, default_case, .. } => {
                self.collect_expr(expression);
                for case in cases {
                    for s in &case.statements { self.collect_from_statement(s); }
                }
                if let Some(def) = default_case {
                    for s in &def.statements { self.collect_from_statement(s); }
                }
            }
            QuickFuncStatement::Log { value, .. } => self.collect_expr(value),
            QuickFuncStatement::ExpressionStatement { expression, .. } => self.collect_expr(expression),
        }
    }

    fn collect_expr(&mut self, expr: &Expression) {
        match expr {
            Expression::Identifier { name, .. } => self.add_ref(name),
            Expression::QualifiedIdentifier { parts, arguments, .. } => {
                if let Some(first) = parts.first() { self.add_ref(first); }
                if let Some(args) = arguments {
                    for a in args { self.collect_expr(a); }
                }
            }
            Expression::ArithmeticOp { left, right, .. }
            | Expression::ComparisonOp { left, right, .. }
            | Expression::LogicalOp { left, right, .. }
            | Expression::BitwiseOp { left, right, .. } => {
                self.collect_expr(left);
                self.collect_expr(right);
            }
            Expression::UnaryOp { operand, .. } => self.collect_expr(operand),
            Expression::Conditional { condition, true_value, false_value, .. } => {
                self.collect_expr(condition);
                self.collect_expr(true_value);
                self.collect_expr(false_value);
            }
            Expression::Parenthesized { expression, .. } => self.collect_expr(expression),
            Expression::PropertyAccess { object, .. } => self.collect_expr(object),
            Expression::IndexAccess { object, index, .. } => {
                self.collect_expr(object);
                self.collect_expr(index);
            }
            Expression::QuickFuncCall { arguments, .. }
            | Expression::ImportedFunctionCall { arguments, .. }
            | Expression::StaticMethodCall { arguments, .. } => {
                for a in arguments { self.collect_expr(a); }
            }
            Expression::InstanceMethodCall { instance, arguments, .. } => {
                self.collect_expr(instance);
                for a in arguments { self.collect_expr(a); }
            }
            Expression::Value { value, .. } => self.collect_value(value),
            Expression::TypeCast { expression, .. } => self.collect_expr(expression),
            _ => {}
        }
    }

    fn collect_value(&mut self, value: &Value) {
        match value {
            Value::Array { values, .. } | Value::NestedArray { values, .. } => {
                for v in values { self.collect_value(v); }
            }
            Value::Object { properties, .. } => {
                for p in properties { self.collect_value(&p.value); }
            }
            Value::PrefixedConstructor { arguments, .. } => {
                for a in arguments { self.collect_value(a); }
            }
            Value::InterpolatedString { expressions, .. } => {
                for e in expressions { self.collect_expr(e); }
            }
            Value::QuickFuncCall { arguments, .. } => {
                for a in arguments { self.collect_expr(a); }
            }
            Value::Expression { expr, .. } => self.collect_expr(expr),
            Value::Lambda { body, .. } => self.collect_expr(body),
            Value::Range { start, end, .. } => {
                self.collect_value(start);
                self.collect_value(end);
            }
            Value::Identifier { value, .. } => self.add_ref(value),
            _ => {}
        }
    }

    #[inline]
    fn add_ref(&mut self, name: &str) {
        if !self.parameters.contains(name) {
            self.referenced.insert(name.to_string());
        }
    }
}
