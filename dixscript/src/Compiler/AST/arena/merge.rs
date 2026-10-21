// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/arena-ast.md, section "Compiler/AST/arena/merge.rs"
// ============================================================================
//! Appending one arena to another, and shifting the ids of the sections that were built in it.
//!
//! The section parsers run in parallel, so each one builds into an arena of its own. To hand a
//! single [`Document`](super::Document) onward, those arenas have to become one. [`Arena::append`]
//! copies every store of one arena onto the end of another and returns an [`ArenaOffsets`] that
//! says how far each store moved. A node that was built in the appended arena names its children
//! by position in that arena's stores, so every id and span in it must move by the same amount:
//! the nodes are rewritten as they are copied, and the `Doc*` section that was built in the
//! appended arena is rewritten with its `shifted` method.
//!
//! Every `match` lists every variant with no wildcard arm, so a new node variant stops the build
//! until its ids are handled here.

use super::{
    Arena, CaseId, CaseList, DocData, DocDataEntry, DocPropertyAssignment, DocQuickFuncParam,
    DocQuickFuncs, DocQuickFunction, DocRawBlock, DocRawField, DocSchema, DocSchemaField,
    DocSecurity, DocSecurityEntry, DocSecurityField, ExprId, ExprList, ExprNode, ObjectPropNode,
    PropList, Str, StmtId, StmtList, StmtNode, StrList, SwitchCaseNode, UnknownId, ValueId,
    ValueList, ValueNode, to_u32,
};

fn add(base: u32, offset: u32) -> u32 {
    base.checked_add(offset).expect("arena exceeds the u32 id space")
}

/// How far each store of an arena moved when it was appended to another arena.
///
/// [`Arena::append`] returns one. Its methods move an id or a span that was issued by the
/// appended arena so that it names the same node in the arena it was appended to, and the
/// `shifted` method of each `Doc*` section applies it to a whole section. An id or span that was
/// issued by the arena that was appended to does not move and must not be passed to these
/// methods.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArenaOffsets {
    values: u32,
    exprs: u32,
    stmts: u32,
    props: u32,
    cases: u32,
    unknowns: u32,
    value_lists: u32,
    expr_lists: u32,
    stmt_lists: u32,
    str_lists: u32,
    text: u32,
}

impl ArenaOffsets {
    /// Moves a value id.
    #[inline]
    pub fn value(self, id: ValueId) -> ValueId {
        ValueId(add(id.0, self.values))
    }

    /// Moves an expression id.
    #[inline]
    pub fn expr(self, id: ExprId) -> ExprId {
        ExprId(add(id.0, self.exprs))
    }

    /// Moves a statement id.
    #[inline]
    pub fn stmt(self, id: StmtId) -> StmtId {
        StmtId(add(id.0, self.stmts))
    }

    /// Moves a switch case id.
    #[inline]
    pub fn case(self, id: CaseId) -> CaseId {
        CaseId(add(id.0, self.cases))
    }

    /// Moves an unknown-element id.
    #[inline]
    pub fn unknown(self, id: UnknownId) -> UnknownId {
        UnknownId(add(id.0, self.unknowns))
    }

    /// Moves a span of text.
    #[inline]
    pub fn text(self, s: Str) -> Str {
        Str { start: add(s.start, self.text), len: s.len }
    }

    /// Moves a run of names.
    #[inline]
    pub fn str_list(self, list: StrList) -> StrList {
        StrList { start: add(list.start, self.str_lists), len: list.len }
    }

    /// Moves a run of values.
    #[inline]
    pub fn value_list(self, list: ValueList) -> ValueList {
        ValueList { start: add(list.start, self.value_lists), len: list.len }
    }

    /// Moves a run of expressions.
    #[inline]
    pub fn expr_list(self, list: ExprList) -> ExprList {
        ExprList { start: add(list.start, self.expr_lists), len: list.len }
    }

    /// Moves a run of statements.
    #[inline]
    pub fn stmt_list(self, list: StmtList) -> StmtList {
        StmtList { start: add(list.start, self.stmt_lists), len: list.len }
    }

    /// Moves a run of object properties.
    #[inline]
    pub fn prop_list(self, list: PropList) -> PropList {
        PropList { start: add(list.start, self.props), len: list.len }
    }

    /// Moves a run of switch cases.
    #[inline]
    pub fn case_list(self, list: CaseList) -> CaseList {
        CaseList { start: add(list.start, self.cases), len: list.len }
    }
}

impl Arena {
    /// Copies every node, list and string of `other` onto the end of this arena and returns how
    /// far each store of `other` moved.
    ///
    /// The nodes are rewritten as they are copied, so the copies name each other and not the
    /// nodes of `other`. Ids and sections that were built in `other` keep naming `other`'s nodes
    /// until they are moved with the returned [`ArenaOffsets`] (for a section, its `shifted`
    /// method). Nothing that was already in this arena changes, so every id it issued stays valid.
    ///
    /// # Panics
    ///
    /// Panics if a store would grow past what a `u32` id can address.
    ///
    /// # Examples
    ///
    /// ```
    /// use dixscript::Compiler::AST::arena::{Arena, DocData, DocDataEntry};
    /// use dixscript::Compiler::AST::{DataEntry, DataSection, Position, Value};
    ///
    /// fn section(n: i32) -> DataSection {
    ///     DataSection::new(
    ///         vec![DataEntry::SimpleProperty {
    ///             name: "n".to_string(),
    ///             data_type: None,
    ///             value: Value::Integer { value: n, position: Position::new(2, 5) },
    ///             position: Position::new(2, 1),
    ///         }],
    ///         Position::new(1, 1),
    ///     )
    /// }
    ///
    /// // Two sections, each built in an arena of its own.
    /// let (mut first, mut second) = (Arena::new(), Arena::new());
    /// let a = DocData::from_owned(&mut first, &section(1));
    /// let b = DocData::from_owned(&mut second, &section(2));
    ///
    /// // Append the second arena to the first and move the second section with it.
    /// let offsets = first.append(&second);
    /// let b = b.shifted(offsets);
    ///
    /// assert_eq!(a.export(&first).to_string(), section(1).to_string());
    /// assert_eq!(b.export(&first).to_string(), section(2).to_string());
    /// ```
    pub fn append(&mut self, other: &Arena) -> ArenaOffsets {
        let offsets = ArenaOffsets {
            values: to_u32(self.values.len()),
            exprs: to_u32(self.exprs.len()),
            stmts: to_u32(self.stmts.len()),
            props: to_u32(self.props.len()),
            cases: to_u32(self.cases.len()),
            unknowns: to_u32(self.unknowns.len()),
            value_lists: to_u32(self.value_lists.len()),
            expr_lists: to_u32(self.expr_lists.len()),
            stmt_lists: to_u32(self.stmt_lists.len()),
            str_lists: to_u32(self.str_lists.len()),
            text: to_u32(self.text.len()),
        };

        self.values.extend(other.values.iter().map(|node| node.shifted(offsets)));
        self.exprs.extend(other.exprs.iter().map(|node| node.shifted(offsets)));
        self.stmts.extend(other.stmts.iter().map(|node| node.shifted(offsets)));
        self.props.extend(other.props.iter().map(|node| node.shifted(offsets)));
        self.cases.extend(other.cases.iter().map(|node| node.shifted(offsets)));
        self.unknowns.extend(other.unknowns.iter().cloned());
        self.value_lists.extend(other.value_lists.iter().map(|id| offsets.value(*id)));
        self.expr_lists.extend(other.expr_lists.iter().map(|id| offsets.expr(*id)));
        self.stmt_lists.extend(other.stmt_lists.iter().map(|id| offsets.stmt(*id)));
        self.str_lists.extend(other.str_lists.iter().map(|s| offsets.text(*s)));
        self.text.push_str(&other.text);

        offsets
    }
}

// -- nodes --------------------------------------------------------------------

impl ValueNode {
    fn shifted(&self, o: ArenaOffsets) -> ValueNode {
        match *self {
            ValueNode::Integer { .. }
            | ValueNode::Long { .. }
            | ValueNode::Float { .. }
            | ValueNode::Double { .. }
            | ValueNode::ScientificNotation { .. }
            | ValueNode::Boolean { .. }
            | ValueNode::Null { .. } => *self,
            ValueNode::String { value, position } => {
                ValueNode::String { value: o.text(value), position }
            }
            ValueNode::InterpolatedString { template, expressions, position } => {
                ValueNode::InterpolatedString {
                    template: o.text(template),
                    expressions: o.expr_list(expressions),
                    position,
                }
            }
            ValueNode::HexColor { value, position } => {
                ValueNode::HexColor { value: o.text(value), position }
            }
            ValueNode::Date { value, position } => {
                ValueNode::Date { value: o.text(value), position }
            }
            ValueNode::Timestamp { value, position } => {
                ValueNode::Timestamp { value: o.text(value), position }
            }
            ValueNode::Array { values, position } => {
                ValueNode::Array { values: o.value_list(values), position }
            }
            ValueNode::NestedArray { values, level, position } => {
                ValueNode::NestedArray { values: o.value_list(values), level, position }
            }
            ValueNode::Object { properties, position } => {
                ValueNode::Object { properties: o.prop_list(properties), position }
            }
            ValueNode::PrefixedConstructor { prefix, arguments, position } => {
                ValueNode::PrefixedConstructor {
                    prefix: o.text(prefix),
                    arguments: o.value_list(arguments),
                    position,
                }
            }
            ValueNode::EnumValue { enum_name, value, position } => ValueNode::EnumValue {
                enum_name: o.text(enum_name),
                value: o.text(value),
                position,
            },
            ValueNode::Identifier { value, position } => {
                ValueNode::Identifier { value: o.text(value), position }
            }
            ValueNode::QuickFuncCall { function_name, arguments, position } => {
                ValueNode::QuickFuncCall {
                    function_name: o.text(function_name),
                    arguments: o.expr_list(arguments),
                    position,
                }
            }
            ValueNode::Expression { expr, position } => {
                ValueNode::Expression { expr: o.expr(expr), position }
            }
            ValueNode::Range { start, end, position } => {
                ValueNode::Range { start: o.value(start), end: o.value(end), position }
            }
            ValueNode::Lambda { parameters, body, statements, position } => ValueNode::Lambda {
                parameters: o.str_list(parameters),
                body: o.expr(body),
                statements: o.stmt_list(statements),
                position,
            },
            ValueNode::ParseError { message, position } => {
                ValueNode::ParseError { message: o.text(message), position }
            }
            ValueNode::Error { message, position } => {
                ValueNode::Error { message: o.text(message), position }
            }
            ValueNode::Unknown { data, position } => {
                ValueNode::Unknown { data: o.unknown(data), position }
            }
        }
    }
}

impl ExprNode {
    fn shifted(&self, o: ArenaOffsets) -> ExprNode {
        match *self {
            ExprNode::Identifier { name, position } => {
                ExprNode::Identifier { name: o.text(name), position }
            }
            ExprNode::QualifiedIdentifier { parts, arguments, position } => {
                ExprNode::QualifiedIdentifier {
                    parts: o.str_list(parts),
                    arguments: arguments.map(|list| o.expr_list(list)),
                    position,
                }
            }
            ExprNode::FunctionCall { name, arguments, position } => ExprNode::FunctionCall {
                name: o.text(name),
                arguments: o.expr_list(arguments),
                position,
            },
            ExprNode::QuickFuncCall { name, arguments, position } => ExprNode::QuickFuncCall {
                name: o.text(name),
                arguments: o.expr_list(arguments),
                position,
            },
            ExprNode::DixFunctionCall { function_name, arguments, position } => {
                ExprNode::DixFunctionCall {
                    function_name: o.text(function_name),
                    arguments: o.expr_list(arguments),
                    position,
                }
            }
            ExprNode::StaticMethodCall { object_name, method_name, arguments, position } => {
                ExprNode::StaticMethodCall {
                    object_name: o.text(object_name),
                    method_name: o.text(method_name),
                    arguments: o.expr_list(arguments),
                    position,
                }
            }
            ExprNode::InstanceMethodCall { instance, method_name, arguments, position } => {
                ExprNode::InstanceMethodCall {
                    instance: o.expr(instance),
                    method_name: o.text(method_name),
                    arguments: o.expr_list(arguments),
                    position,
                }
            }
            ExprNode::BuiltinFunction { target, method, arguments, position } => {
                ExprNode::BuiltinFunction {
                    target: o.expr(target),
                    method: o.text(method),
                    arguments: arguments.map(|list| o.expr_list(list)),
                    position,
                }
            }
            ExprNode::StaticFunction { class_name, method, arguments, position } => {
                ExprNode::StaticFunction {
                    class_name: o.text(class_name),
                    method: o.text(method),
                    arguments: o.expr_list(arguments),
                    position,
                }
            }
            ExprNode::ImportedFunctionCall { namespace_name, function_name, arguments, position } => {
                ExprNode::ImportedFunctionCall {
                    namespace_name: o.text(namespace_name),
                    function_name: o.text(function_name),
                    arguments: o.expr_list(arguments),
                    position,
                }
            }
            ExprNode::ArithmeticOp { left, operator, right, position } => {
                ExprNode::ArithmeticOp {
                    left: o.expr(left),
                    operator: o.text(operator),
                    right: o.expr(right),
                    position,
                }
            }
            ExprNode::BitwiseOp { left, operator, right, position } => ExprNode::BitwiseOp {
                left: o.expr(left),
                operator: o.text(operator),
                right: o.expr(right),
                position,
            },
            ExprNode::ComparisonOp { left, operator, right, position } => {
                ExprNode::ComparisonOp {
                    left: o.expr(left),
                    operator: o.text(operator),
                    right: o.expr(right),
                    position,
                }
            }
            ExprNode::LogicalOp { left, operator, right, position } => ExprNode::LogicalOp {
                left: o.expr(left),
                operator: o.text(operator),
                right: o.expr(right),
                position,
            },
            ExprNode::UnaryOp { operator, operand, position } => ExprNode::UnaryOp {
                operator: o.text(operator),
                operand: o.expr(operand),
                position,
            },
            ExprNode::ConfigAccess { key, position } => {
                ExprNode::ConfigAccess { key: o.text(key), position }
            }
            ExprNode::EnumAccess { namespace_name, enum_name, value, position } => {
                ExprNode::EnumAccess {
                    namespace_name: namespace_name.map(|s| o.text(s)),
                    enum_name: o.text(enum_name),
                    value: o.text(value),
                    position,
                }
            }
            ExprNode::ObjectAccess { path, position } => {
                ExprNode::ObjectAccess { path: o.str_list(path), position }
            }
            ExprNode::PropertyAccess { object, property, position } => {
                ExprNode::PropertyAccess {
                    object: o.expr(object),
                    property: o.text(property),
                    position,
                }
            }
            ExprNode::IndexAccess { object, index, position } => {
                ExprNode::IndexAccess { object: o.expr(object), index: o.expr(index), position }
            }
            ExprNode::Value { value, position } => {
                ExprNode::Value { value: o.value(value), position }
            }
            ExprNode::Parenthesized { expression, position } => {
                ExprNode::Parenthesized { expression: o.expr(expression), position }
            }
            ExprNode::Conditional { condition, true_value, false_value, position } => {
                ExprNode::Conditional {
                    condition: o.expr(condition),
                    true_value: o.expr(true_value),
                    false_value: o.expr(false_value),
                    position,
                }
            }
            ExprNode::TypeCast { expression, target_type, position } => ExprNode::TypeCast {
                expression: o.expr(expression),
                target_type,
                position,
            },
        }
    }
}

impl StmtNode {
    fn shifted(&self, o: ArenaOffsets) -> StmtNode {
        match *self {
            StmtNode::Return { value, position } => {
                StmtNode::Return { value: o.expr(value), position }
            }
            StmtNode::If { condition, then_branch, else_branch, position } => StmtNode::If {
                condition: o.expr(condition),
                then_branch: o.stmt_list(then_branch),
                else_branch: else_branch.map(|list| o.stmt_list(list)),
                position,
            },
            StmtNode::Switch { expression, cases, default_case, position } => StmtNode::Switch {
                expression: o.expr(expression),
                cases: o.case_list(cases),
                default_case: default_case.map(|id| o.case(id)),
                position,
            },
            StmtNode::Assignment { variable, value, position } => StmtNode::Assignment {
                variable: o.text(variable),
                value: o.expr(value),
                position,
            },
            StmtNode::ArithmeticAssignment { variable, operator, value, position } => {
                StmtNode::ArithmeticAssignment {
                    variable: o.text(variable),
                    operator: o.text(operator),
                    value: o.expr(value),
                    position,
                }
            }
            StmtNode::ObjectCreation { variable, object, position } => {
                StmtNode::ObjectCreation {
                    variable: o.text(variable),
                    object: o.value(object),
                    position,
                }
            }
            StmtNode::Log { value, position } => StmtNode::Log { value: o.expr(value), position },
            StmtNode::ExpressionStatement { expression, position } => {
                StmtNode::ExpressionStatement { expression: o.expr(expression), position }
            }
            StmtNode::VariableDeclaration {
                declaration_type,
                is_mutable,
                variable_name,
                data_type,
                value,
                position,
            } => StmtNode::VariableDeclaration {
                declaration_type,
                is_mutable,
                variable_name: o.text(variable_name),
                data_type,
                value: o.expr(value),
                position,
            },
        }
    }
}

impl ObjectPropNode {
    fn shifted(&self, o: ArenaOffsets) -> ObjectPropNode {
        ObjectPropNode { key: o.text(self.key), value: o.value(self.value), position: self.position }
    }
}

impl SwitchCaseNode {
    fn shifted(&self, o: ArenaOffsets) -> SwitchCaseNode {
        SwitchCaseNode {
            case_value: o.value(self.case_value),
            statements: o.stmt_list(self.statements),
            position: self.position,
        }
    }
}

// -- sections -----------------------------------------------------------------

impl DocData {
    /// Moves every id of this section by `offsets`, for a section that was built in an arena
    /// that has since been appended to another with [`Arena::append`].
    pub fn shifted(self, offsets: ArenaOffsets) -> DocData {
        DocData {
            entries: self.entries.into_iter().map(|entry| entry.shifted(offsets)).collect(),
            position: self.position,
        }
    }
}

impl DocDataEntry {
    fn shifted(self, o: ArenaOffsets) -> DocDataEntry {
        match self {
            DocDataEntry::SimpleProperty { name, data_type, value, position } => {
                DocDataEntry::SimpleProperty { name, data_type, value: o.value(value), position }
            }
            DocDataEntry::TableProperty { path, properties, position } => {
                DocDataEntry::TableProperty {
                    path,
                    properties: properties
                        .into_iter()
                        .map(|p| DocPropertyAssignment { value: o.value(p.value), ..p })
                        .collect(),
                    position,
                }
            }
            DocDataEntry::GroupArray { path, items, position } => {
                DocDataEntry::GroupArray { path, items: o.value_list(items), position }
            }
            DocDataEntry::ObjectProperty { name, data_type, object, position } => {
                DocDataEntry::ObjectProperty { name, data_type, object: o.value(object), position }
            }
        }
    }
}

impl DocRawBlock {
    /// Moves every id of this block by `offsets`, for a block that was built in an arena that has
    /// since been appended to another with [`Arena::append`].
    pub fn shifted(self, offsets: ArenaOffsets) -> DocRawBlock {
        let shift = |fields: Vec<DocRawField>| -> Vec<DocRawField> {
            fields
                .into_iter()
                .map(|f| DocRawField { value: offsets.value(f.value), ..f })
                .collect()
        };
        DocRawBlock {
            meta_data: shift(self.meta_data),
            using: shift(self.using),
            content: self.content,
            position: self.position,
        }
    }
}

impl DocSecurity {
    /// Moves every id of this section by `offsets`, for a section that was built in an arena
    /// that has since been appended to another with [`Arena::append`].
    pub fn shifted(self, offsets: ArenaOffsets) -> DocSecurity {
        DocSecurity {
            entries: self
                .entries
                .into_iter()
                .map(|entry| DocSecurityEntry {
                    fields: entry
                        .fields
                        .into_iter()
                        .map(|f| DocSecurityField { value: offsets.value(f.value), ..f })
                        .collect(),
                    ..entry
                })
                .collect(),
            position: self.position,
        }
    }
}

impl DocSchema {
    /// Moves every id of this section by `offsets`, for a section that was built in an arena
    /// that has since been appended to another with [`Arena::append`].
    pub fn shifted(self, offsets: ArenaOffsets) -> DocSchema {
        DocSchema {
            fields: self
                .fields
                .into_iter()
                .map(|f| DocSchemaField { arguments: offsets.value_list(f.arguments), ..f })
                .collect(),
            position: self.position,
        }
    }
}

impl DocQuickFuncs {
    /// Moves every id of this section by `offsets`, for a section that was built in an arena
    /// that has since been appended to another with [`Arena::append`].
    pub fn shifted(self, offsets: ArenaOffsets) -> DocQuickFuncs {
        DocQuickFuncs {
            functions: self
                .functions
                .into_iter()
                .map(|func| DocQuickFunction {
                    parameters: func
                        .parameters
                        .into_iter()
                        .map(|p| DocQuickFuncParam {
                            default_value: p.default_value.map(|id| offsets.expr(id)),
                            ..p
                        })
                        .collect(),
                    body: offsets.stmt_list(func.body),
                    ..func
                })
                .collect(),
            position: self.position,
        }
    }
}
