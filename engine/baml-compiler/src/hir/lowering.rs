//! AST lowering to HIR.
//!
//! This files contains the convertions between Baml AST nodes to HIR nodes.

use std::collections::{HashMap, HashSet};

use baml_types::{
    type_meta::{self, base::StreamingBehavior},
    Constraint, ConstraintLevel, TypeIR, TypeValue,
};
use internal_baml_ast::ast::{self, App, AssertStmt, Attribute, ReturnStmt, WithName, WithSpan};
use internal_baml_diagnostics::Span;

use crate::{
    hir::{
        self, Block, Class, ClassConstructor, ClassConstructorField, Enum, EnumVariant,
        ExprFunction, Expression, Field, HeaderContext, Hir, LlmFunction, Parameter, Statement,
        TypeArg,
    },
    watch::{WatchSpec, WatchWhen},
};

impl Hir {
    /// Lower BAML AST into HIR.
    pub fn from_ast(ast: &ast::Ast) -> Self {
        let mut hir = Self {
            expr_functions: vec![],
            llm_functions: vec![],
            classes: vec![],
            enums: vec![],
            global_assignments: baml_types::BamlMap::new(),
        };

        // Add builtin classes and enums first
        hir.classes.extend(crate::builtin::builtin_classes());
        hir.enums.extend(crate::builtin::builtin_enums());

        for top in &ast.tops {
            match top {
                ast::Top::Function(function) => {
                    hir.llm_functions.push(LlmFunction::from_ast(function))
                }
                ast::Top::ExprFn(expr_fn) => {
                    hir.expr_functions.push(ExprFunction::from_ast(expr_fn))
                }
                ast::Top::Class(class) => hir.classes.push(Class::from_ast(class)),
                ast::Top::Enum(enum_def) => hir.enums.push(Enum::from_ast(enum_def)),
                ast::Top::TopLevelAssignment(assignment) => {
                    // Add toplevel assignments to global_assignments for HIR typechecking
                    let value = Expression::from_ast(&assignment.stmt.expr);
                    let annotated_type = assignment.stmt.annotation.as_ref().map(type_ir_from_ast);
                    hir.global_assignments.insert(
                        assignment.stmt.identifier.to_string(),
                        crate::hir::GlobalAssignment {
                            value,
                            annotated_type,
                            span: assignment.stmt.span.clone(),
                        },
                    );
                }
                _ => {}
            }
        }

        let enums = HashSet::<&str>::from_iter(hir.enums.iter().map(|e| e.name.as_str()));

        let param_type: fn(&mut Parameter) -> &mut TypeIR = |p| &mut p.r#type;

        // Patch return types because only here in the code we have the full
        // context for enums.
        hir.expr_functions
            .iter_mut()
            .map(|f| (f.parameters.iter_mut().map(param_type), &mut f.return_type))
            .chain(
                hir.llm_functions
                    .iter_mut()
                    .map(|f| (f.parameters.iter_mut().map(param_type), &mut f.return_type)),
            )
            .chain(hir.classes.iter_mut().flat_map(|c| {
                c.methods
                    .iter_mut()
                    .map(|f| (f.parameters.iter_mut().map(param_type), &mut f.return_type))
            }))
            .flat_map(|(parameters, return_type)| parameters.chain(std::iter::once(return_type)))
            .for_each(|ty| match ty {
                TypeIR::Class { name, meta, .. } if enums.contains(name.as_str()) => {
                    *ty = TypeIR::Enum {
                        name: name.to_owned(),
                        dynamic: false, // TODO: How to know if it's dynamic.
                        meta: meta.clone(),
                    }
                }

                _ => {}
            });

        hir
    }

    #[cfg(test)]
    /// Test helper to generate HIR from BAML source.
    pub fn from_source(source: &'static str) -> Self {
        let parser_db = crate::test::ast(source).unwrap_or_else(|e| panic!("{}", e));
        Hir::from_ast(&parser_db.ast)
    }
}

pub fn type_ir_from_ast_optional(r#type: Option<&ast::FieldType>) -> TypeIR {
    match r#type {
        Some(r#type) => type_ir_from_ast(r#type),
        None => TypeIR::null(),
    }
}

pub fn type_ir_from_ast(type_: &ast::FieldType) -> TypeIR {
    let mut constraints = Vec::new();
    let mut streaming_behavior = StreamingBehavior::default();

    // Convert attributes to constraints and streaming behavior
    type_.attributes().iter().for_each(|attr: &Attribute| {
        match attr.name.name() {
            // Handle constraint attributes
            "assert" | "check" => {
                let level = match attr.name.name() {
                    "assert" => ConstraintLevel::Assert,
                    "check" => ConstraintLevel::Check,
                    _ => unreachable!(),
                };

                // Extract label and expression from arguments
                let arguments: Vec<&ast::Expression> = attr
                    .arguments
                    .arguments
                    .iter()
                    .map(|arg| &arg.value)
                    .collect();

                let (label, expression) = match arguments.as_slice() {
                    // Single argument: just the expression
                    [ast::Expression::JinjaExpressionValue(jinja_expr, _)] => {
                        (None, Some(jinja_expr.clone()))
                    }
                    // Two arguments: label and expression
                    [
                        ast::Expression::Identifier(label_id),
                        ast::Expression::JinjaExpressionValue(jinja_expr, _),
                    ] => (Some(label_id.to_string()), Some(jinja_expr.clone())),
                    _ => {
                        // Skip invalid constraint formats
                        (None, None)
                    }
                };

                if let Some(expr) = expression {
                    constraints.push(Constraint {
                        level,
                        expression: expr,
                        label,
                    });
                }
            }
            // Handle streaming behavior attributes
            "stream.not_null" => {
                streaming_behavior.needed = true;
            }
            "stream.done" => {
                streaming_behavior.done = true;
            }
            "stream.with_state" => {
                streaming_behavior.state = true;
            }
            _ => {
                // Ignore other attributes for now
            }
        }
    });

    let meta = type_meta::IR {
        constraints,
        streaming_behavior,
    };

    let base_type = match type_ {
        ast::FieldType::Symbol(arity, name, _) => {
            let base = TypeIR::Class {
                name: name.name().to_string(),
                mode: baml_types::ir_type::StreamingMode::NonStreaming,
                dynamic: false,
                meta,
            };
            if arity.is_optional() {
                TypeIR::optional(base)
            } else {
                base
            }
        }
        ast::FieldType::Primitive(arity, prim, _, _) => {
            let base = TypeIR::Primitive(*prim, meta);
            if arity.is_optional() {
                TypeIR::optional(base)
            } else {
                base
            }
        }
        ast::FieldType::List(arity, inner, dims, _, _) => {
            // Respect multi-dimensional arrays (e.g., int[][] has dims=2)
            let mut lowered_inner = type_ir_from_ast(inner);
            for _ in 0..*dims {
                lowered_inner = TypeIR::List(Box::new(lowered_inner), meta.clone());
            }
            if arity.is_optional() {
                TypeIR::optional(lowered_inner)
            } else {
                lowered_inner
            }
        }
        ast::FieldType::Map(arity, box_pair, _, _) => {
            let base = TypeIR::Map(
                Box::new(type_ir_from_ast(&box_pair.0)),
                Box::new(type_ir_from_ast(&box_pair.1)),
                meta,
            );
            if arity.is_optional() {
                TypeIR::optional(base)
            } else {
                base
            }
        }
        ast::FieldType::Union(arity, types, _, _) => {
            let union_types: Vec<TypeIR> = types.iter().map(type_ir_from_ast).collect();
            let base = TypeIR::union_with_meta(union_types, meta);
            if arity.is_optional() {
                TypeIR::optional(base)
            } else {
                base
            }
        }
        _ => TypeIR::Primitive(TypeValue::String, meta), // Default case for other variants
    };
    base_type
}

/// Is the type complex enough that it should be parenthesized if it's not
/// top-level?
pub fn complex(type_: &TypeIR) -> bool {
    let meta = type_.meta();
    if meta.streaming_behavior != StreamingBehavior::default() {
        return true;
    }
    if !meta.constraints.is_empty() {
        return true;
    }
    match type_ {
        TypeIR::Union(_, _) => true,
        TypeIR::Primitive(baml_types::TypeValue::Int, _) => false,
        TypeIR::Primitive(baml_types::TypeValue::Float, _) => false,
        TypeIR::Primitive(baml_types::TypeValue::String, _) => false,
        TypeIR::Primitive(baml_types::TypeValue::Bool, _) => false,
        TypeIR::List(_, _) => false,
        TypeIR::Map(_, _, _) => false,
        TypeIR::Class { .. } => false,
        TypeIR::Enum { .. } => false,
        TypeIR::Primitive(baml_types::TypeValue::Null, _) => false,
        TypeIR::Arrow(_, _) => true,
        _ => false,
    }
}

impl LlmFunction {
    pub fn from_ast(function: &ast::ValueExprBlock) -> Self {
        LlmFunction {
            name: function.name().to_string(),
            parameters: function.input().map(lower_fn_args).unwrap_or_default(),

            return_type: type_ir_from_ast_optional(
                function.output().map(|output| &output.field_type),
            ),
            // return_type: TypeIR::from_ast(function.output().unwrap_or(&FieldType::Primitive(
            //     FieldArity::Required,
            //     TypeValue::Null,
            //     Span::fake(),
            //     None,
            // ))),
            client: function
                .fields()
                .iter()
                .find(|attr| attr.name() == "client")
                .map(|attr| {
                    attr.expr
                        .as_ref()
                        .expect("client must be specified")
                        .to_string()
                })
                .unwrap_or("llm".to_string()),
            prompt: function
                .fields()
                .iter()
                .find(|attr| attr.name() == "prompt")
                .map(|attr| {
                    attr.expr
                        .as_ref()
                        .expect("prompt must be specified")
                        .to_string()
                })
                .unwrap_or("".to_string()),
            span: function.span().clone(),
        }
    }
}

fn lower_fn_args(input: &ast::BlockArgs) -> Vec<Parameter> {
    input
        .args
        .iter()
        .map(|(name, param)| Parameter {
            name: name.to_string(),
            is_mutable: param.is_mutable, // Will always be true from parser after mut keyword removal
            r#type: type_ir_from_ast(&param.field_type),
            span: name.span().clone(),
        })
        .collect::<Vec<_>>()
}

impl ExprFunction {
    /// Lower an expression function into HIR.
    pub fn from_ast(function: &ast::ExprFn) -> Self {
        ExprFunction {
            name: function.name.to_string(),
            parameters: lower_fn_args(&function.args),
            return_type: type_ir_from_ast_optional(function.return_type.as_ref()),
            body: Block::from_expr_block(&function.body),
            span: function.span.clone(),
        }
    }
}

/// Extract name and when fields from a WatchOptions class constructor expression.
/// Expected expression: baml.WatchOptions{name: "value", when: FunctionName}
fn extract_watch_options_fields(expr: &ast::Expression) -> (Option<String>, Option<WatchWhen>) {
    use ast::Expression;

    // The expression should be a class constructor
    if let Expression::ClassConstructor(class_ctor, _) = expr {
        let mut name = None;
        let mut when = None;

        // Extract field values
        for field in &class_ctor.fields {
            if let ast::ClassConstructorField::Named(field_name, field_value) = field {
                match field_name.name() {
                    "channel" => {
                        // name should be a string value
                        if let Expression::StringValue(s, _) = field_value {
                            name = Some(s.clone());
                        } else if let Expression::RawStringValue(raw) = field_value {
                            name = Some(raw.value().to_string());
                        }
                    }
                    "when" => {
                        // when should be an identifier (function name) or string "manual"
                        match field_value {
                            Expression::Identifier(id) => {
                                when = Some(WatchWhen::FunctionName(id.clone()));
                            }
                            Expression::StringValue(s, _) if s == "manual" => {
                                when = Some(WatchWhen::Manual);
                            }
                            Expression::StringValue(s, _) if s == "never" => {
                                when = Some(WatchWhen::Never);
                            }
                            _ => {}
                        }
                    }
                    _ => {} // Ignore unknown fields
                }
            }
        }

        (name, when)
    } else {
        // If not a class constructor, return empty options
        (None, None)
    }
}

impl Block {
    /// Lower an expression block into HIR for expression blocks.
    pub fn from_expr_block(block: &ast::ExpressionBlock) -> Self {
        // First pass: collect all WatchOptions statements into a map
        let mut watch_options_map: HashMap<String, (Option<String>, Option<WatchWhen>)> =
            HashMap::new();
        for stmt in &block.stmts {
            if let ast::Stmt::WatchOptions(ast::WatchOptionsStmt {
                variable,
                options_expr,
                ..
            }) = stmt
            {
                // Extract name and when from the WatchOptions class constructor expression
                let (channel, when) = extract_watch_options_fields(options_expr);
                watch_options_map.insert(variable.to_string(), (channel, when));
            }
        }

        // Second pass: lower statements, applying watch options to watch specs
        let mut statements: Vec<Statement> = Vec::new();
        for stmt in &block.stmts {
            statements.extend(lower_stmt_with_options(stmt, &watch_options_map));
        }

        let trailing_expr = block
            .expr
            .as_deref()
            .map(Expression::from_ast)
            .map(Box::new);

        if !block.expr_headers.is_empty() {
            statements.extend(header_context_statements(&block.expr_headers));
        }

        Block {
            statements,
            trailing_expr,
        }
    }
}

fn header_context_statements(headers: &[std::sync::Arc<ast::Header>]) -> Vec<Statement> {
    headers
        .iter()
        .map(|header| {
            Statement::HeaderContextEnter(HeaderContext {
                level: header.level,
                title: header.title.clone(),
                span: header.span.clone(),
            })
        })
        .collect()
}

fn wrap_statements_as_expression_block(statements: Vec<Statement>, span: Span) -> Statement {
    Statement::Expression {
        expr: Expression::Block(
            Block {
                statements,
                trailing_expr: None,
            },
            span.clone(),
        ),
        span,
    }
}

fn lower_stmt(stmt: &ast::Stmt) -> Vec<Statement> {
    lower_stmt_with_options(stmt, &HashMap::new())
}

#[allow(clippy::ptr_arg)]
fn maybe_annotated_statement(
    stmt: Statement,
    annotated_comments: &[std::sync::Arc<ast::Header>],
) -> Vec<Statement> {
    if annotated_comments.is_empty() {
        vec![stmt]
    } else {
        let mut statements = header_context_statements(annotated_comments);
        statements.push(stmt);
        statements
    }
}

fn lower_stmt_with_options(
    stmt: &ast::Stmt,
    watch_options: &HashMap<String, (Option<String>, Option<WatchWhen>)>,
) -> Vec<Statement> {
    match stmt {
        ast::Stmt::CForLoop(stmt) => {
            let condition = stmt.condition.as_ref().map(Expression::from_ast);
            let init_statements = stmt.init_stmt.as_ref().map(|s| lower_stmt(s));
            let block = Block::from_expr_block(&stmt.body);
            let after_statements = stmt.after_stmt.as_ref().map(|s| lower_stmt(s));

            let after = after_statements.and_then(|stmts| {
                if stmts.is_empty() {
                    None
                } else {
                    Some(Box::new(wrap_statements_as_expression_block(
                        stmts,
                        stmt.span.clone(),
                    )))
                }
            });

            let inner_loop = match (condition, after) {
                (Some(condition), None) => Statement::While {
                    condition,
                    block,
                    span: stmt.span.clone(),
                },
                (condition, after) => Statement::CForLoop {
                    condition,
                    after,
                    block,
                },
            };

            let statement = if let Some(mut init) = init_statements {
                if init.is_empty() {
                    inner_loop
                } else {
                    init.push(inner_loop);
                    wrap_statements_as_expression_block(init, stmt.span.clone())
                }
            } else {
                inner_loop
            };

            let mut statements = header_context_statements(&stmt.annotations);
            statements.push(statement);
            statements
        }
        ast::Stmt::Break(ast::BreakStmt { span, annotations }) => {
            let statement = Statement::Break(span.clone());
            maybe_annotated_statement(statement, annotations)
        }
        ast::Stmt::Continue(ast::ContinueStmt { span, annotations }) => {
            let statement = Statement::Continue(span.clone());
            maybe_annotated_statement(statement, annotations)
        }
        ast::Stmt::WhileLoop(ast::WhileStmt {
            condition,
            body,
            span,
            annotations,
        }) => {
            // lowering to HIR is trivial, since HIR maps 1:1 with this.

            let condition = Expression::from_ast(condition);

            let body = Block::from_expr_block(body);

            let statement = Statement::While {
                condition,
                block: body,
                span: span.clone(),
            };
            maybe_annotated_statement(statement, annotations)
        }
        ast::Stmt::Assign(ast::AssignStmt {
            left,
            expr,
            span,
            annotations,
        }) => {
            let statement = Statement::Assign {
                left: Expression::from_ast(left),
                value: Expression::from_ast(expr),
                span: span.clone(),
            };
            maybe_annotated_statement(statement, annotations)
        }
        ast::Stmt::AssignOp(ast::AssignOpStmt {
            left,
            assign_op,
            expr,
            span,
            annotations,
        }) => {
            let statement = Statement::AssignOp {
                left: Expression::from_ast(left),
                assign_op: match assign_op {
                    ast::AssignOp::AddAssign => hir::AssignOp::AddAssign,
                    ast::AssignOp::SubAssign => hir::AssignOp::SubAssign,
                    ast::AssignOp::MulAssign => hir::AssignOp::MulAssign,
                    ast::AssignOp::DivAssign => hir::AssignOp::DivAssign,
                    ast::AssignOp::ModAssign => hir::AssignOp::ModAssign,
                    ast::AssignOp::BitXorAssign => hir::AssignOp::BitXorAssign,
                    ast::AssignOp::BitAndAssign => hir::AssignOp::BitAndAssign,
                    ast::AssignOp::BitOrAssign => hir::AssignOp::BitOrAssign,
                    ast::AssignOp::ShlAssign => hir::AssignOp::ShlAssign,
                    ast::AssignOp::ShrAssign => hir::AssignOp::ShrAssign,
                },
                value: Expression::from_ast(expr),
                span: span.clone(),
            };
            maybe_annotated_statement(statement, annotations)
        }
        ast::Stmt::Let(ast::LetStmt {
            identifier,
            is_mutable,
            annotation,
            expr,
            span,
            annotations: annotated_comments,
            is_watched,
        }) => {
            let lifted_expr = Expression::from_ast(expr);
            let annotated_type = annotation.as_ref().map(type_ir_from_ast);

            let var_name = identifier.to_string();
            let mut watch_spec = if *is_watched {
                // Create default watch spec - runtime WatchOptions statements will modify it
                Some(WatchSpec::default_for_variable(
                    var_name.clone(),
                    span.clone(),
                ))
            } else {
                None
            };

            if let Some(spec) = watch_spec.as_mut() {
                if let Some((channel, when)) = watch_options.get(&var_name) {
                    if let Some(channel_name) = channel {
                        spec.name = channel_name.clone();
                    }
                    if let Some(when) = when.clone() {
                        spec.when = when;
                    }
                }
            }

            let statement = if *is_mutable {
                Statement::DeclareAndAssign {
                    name: var_name.clone(),
                    value: lifted_expr,
                    annotated_type,
                    watch: watch_spec,
                    span: span.clone(),
                }
            } else {
                Statement::Let {
                    name: var_name.clone(),
                    value: lifted_expr,
                    annotated_type,
                    watch: watch_spec,
                    span: span.clone(),
                }
            };

            maybe_annotated_statement(statement, annotated_comments)
        }
        ast::Stmt::ForLoop(ast::ForLoopStmt {
            identifier,
            iterator,
            body,
            span,
            has_let: _,
            annotations: annotated_comments,
        }) => {
            // Lower for loop to HIR
            let lifted_iterator = Expression::from_ast(iterator);

            // Add the for loop statement
            let statement = Statement::ForLoop {
                identifier: identifier.name().to_string(),
                iterator: Box::new(lifted_iterator),
                block: Block::from_expr_block(body),
                span: span.clone(),
            };

            maybe_annotated_statement(statement, annotated_comments)
        }
        ast::Stmt::Expression(ast::ExprStmt {
            expr,
            span,
            annotations: annotated_comments,
        }) => {
            let statement = Statement::Expression {
                expr: Expression::from_ast(expr),
                span: span.clone(),
            };
            maybe_annotated_statement(statement, annotated_comments)
        }
        ast::Stmt::Semicolon(ast::ExprStmt {
            expr,
            span,
            annotations: annotated_comments,
        }) => {
            let statement = Statement::Semicolon {
                expr: Expression::from_ast(expr),
                span: span.clone(),
            };
            maybe_annotated_statement(statement, annotated_comments)
        }
        ast::Stmt::Return(ReturnStmt {
            value,
            span,
            annotations,
        }) => {
            let statement = Statement::Return {
                expr: Expression::from_ast(value),
                span: span.clone(),
            };
            maybe_annotated_statement(statement, annotations)
        }
        ast::Stmt::Assert(AssertStmt {
            value,
            span,
            annotations,
        }) => {
            let statement = Statement::Assert {
                condition: Expression::from_ast(value),
                span: span.clone(),
            };
            maybe_annotated_statement(statement, annotations)
        }
        ast::Stmt::WatchOptions(ast::WatchOptionsStmt {
            variable,
            options_expr,
            span,
            annotations,
        }) => {
            // Extract name and when from the WatchOptions expression
            let (channel, when) = extract_watch_options_fields(options_expr);
            let statement = Statement::WatchOptions {
                variable: variable.to_string(),
                channel,
                when,
                span: span.clone(),
            };
            maybe_annotated_statement(statement, annotations)
        }
        ast::Stmt::WatchNotify(ast::WatchNotifyStmt {
            variable,
            span,
            annotations,
        }) => {
            let statement = Statement::WatchNotify {
                variable: variable.to_string(),
                span: span.clone(),
            };
            maybe_annotated_statement(statement, annotations)
        }
    }
}

impl Expression {
    /// Lower an expression into HIR.
    ///
    /// If `with_lifting` is true, if expressions will be lifted to temporary variables
    /// and the statements will be added to the provided vector.
    /// If `with_lifting` is false, if expressions will fall back to placeholders.
    pub fn from_ast(expr: &ast::Expression) -> Self {
        match expr {
            ast::Expression::ArrayAccess(base, index, span) => Expression::ArrayAccess {
                base: Box::new(Self::from_ast(base)),
                index: Box::new(Self::from_ast(index)),
                span: span.clone(),
            },
            ast::Expression::FieldAccess(base, field, span) => {
                if let ast::Expression::Identifier(identifier) = base.as_ref() {
                    if identifier.name() == "env" {
                        return Expression::Call {
                            function: Box::new(Expression::Identifier(
                                "env.get".to_string(),
                                identifier.span().clone(),
                            )),
                            type_args: vec![],
                            args: vec![Expression::StringValue(
                                field.name().to_string(),
                                field.span().clone(),
                            )],
                            span: span.clone(),
                        };
                    }
                }

                Expression::FieldAccess {
                    base: Box::new(Self::from_ast(base)),
                    field: field.to_string(),
                    span: span.clone(),
                }
            }
            ast::Expression::MethodCall {
                receiver,
                method,
                args,
                type_args,
                span,
            } => Expression::MethodCall {
                receiver: Box::new(Self::from_ast(receiver)),
                method: method.to_string(),
                args: args.iter().map(Self::from_ast).collect(),
                type_args: type_args
                    .iter()
                    .map(|arg| TypeArg::Type(type_ir_from_ast(arg)))
                    .collect(),
                span: span.clone(),
            },
            ast::Expression::BoolValue(value, span) => Expression::BoolValue(*value, span.clone()),
            ast::Expression::NumericValue(value, span) => {
                Expression::NumericValue(value.to_string(), span.clone())
            }
            ast::Expression::Identifier(identifier) => {
                Expression::Identifier(identifier.to_string(), identifier.span().clone())
            }
            ast::Expression::StringValue(value, span) => {
                Expression::StringValue(value.to_string(), span.clone())
            }
            ast::Expression::RawStringValue(raw_string) => Expression::RawStringValue(
                raw_string.inner_value.to_string(),
                raw_string.span().clone(),
            ),
            ast::Expression::Array(values, span) => {
                Expression::Array(values.iter().map(Self::from_ast).collect(), span.clone())
            }
            ast::Expression::App(App {
                name,
                type_args,
                args,
                span,
            }) => {
                // Note: AST function calls are always just names next to argument lists.
                // Later, we will be able to call any expression that is a function.
                // e.g. foo.name_callback(name),
                // but we don't support this in the AST yet.
                let hir_name = Expression::Identifier(name.to_string(), name.span().clone());
                // Note: User calls of generic functions always use concrete types - this
                // is enforced in the AST. At some point in the future, we may allow the
                // user to instantiate generic functions with type variables. But we don't
                // support this yet.
                let hir_type_args = type_args
                    .iter()
                    .map(|arg| TypeArg::Type(type_ir_from_ast(arg)))
                    .collect();
                Expression::Call {
                    function: Box::new(hir_name),
                    type_args: hir_type_args,
                    args: args.iter().map(Self::from_ast).collect(),
                    span: span.clone(),
                }
            }
            ast::Expression::Map(pairs, span) => Expression::Map(
                pairs
                    .iter()
                    .map(|(key, value)| (Self::from_ast(key), Self::from_ast(value)))
                    .collect(),
                span.clone(),
            ),
            ast::Expression::If(condition, if_branch, else_branch, span) => Expression::If {
                condition: Box::new(Self::from_ast(condition)),
                if_branch: Box::new(Self::from_ast(if_branch)),
                else_branch: else_branch
                    .as_ref()
                    .map(|block| Box::new(Self::from_ast(block))),
                span: span.clone(),
            },
            ast::Expression::ExprBlock(block, span) => {
                // Expression blocks are lowered to HIR preserving their structure
                // This maintains proper scoping - variables defined inside the block
                // are only visible within that block
                Expression::Block(Block::from_expr_block(block), span.clone())
            }
            ast::Expression::Lambda(_, _, _) => {
                todo!("lambdas are not yet implemented")
            }
            ast::Expression::ClassConstructor(cc, span) => {
                // TODO: To handle spreads, if there is a spread, compute a sequence
                // of (field_name, spread_value.field_name) pairs. Use these pairs
                // in the lowering of ClassConstructors, for each field of the class
                // not present in the class constructor.
                //
                // We can't do this yet because we have no syntax for field accessors.

                Expression::ClassConstructor(
                    ClassConstructor {
                        class_name: cc.class_name.to_string(),
                        fields: cc
                            .fields
                            .iter()
                            .map(|field| match field {
                                ast::ClassConstructorField::Named(name, expr) => {
                                    ClassConstructorField::Named {
                                        name: name.to_string(),
                                        value: Self::from_ast(expr),
                                    }
                                }
                                ast::ClassConstructorField::Spread(expr) => {
                                    ClassConstructorField::Spread {
                                        value: Self::from_ast(expr),
                                    }
                                }
                            })
                            .collect(),
                    },
                    span.clone(),
                )
            }
            ast::Expression::JinjaExpressionValue(jinja, span) => {
                Expression::JinjaExpressionValue(jinja.to_string(), span.clone())
            }
            ast::Expression::BinaryOperation {
                left,
                operator,
                right,
                span,
            } => Expression::BinaryOperation {
                left: Box::new(Self::from_ast(left)),
                // TODO: Looks kind of redundant, maybe we can make a module
                // for reusable structs in both AST and HIR.
                operator: match operator {
                    ast::BinaryOperator::Eq => hir::BinaryOperator::Eq,
                    ast::BinaryOperator::Neq => hir::BinaryOperator::Neq,
                    ast::BinaryOperator::Lt => hir::BinaryOperator::Lt,
                    ast::BinaryOperator::LtEq => hir::BinaryOperator::LtEq,
                    ast::BinaryOperator::Gt => hir::BinaryOperator::Gt,
                    ast::BinaryOperator::GtEq => hir::BinaryOperator::GtEq,
                    ast::BinaryOperator::Add => hir::BinaryOperator::Add,
                    ast::BinaryOperator::Sub => hir::BinaryOperator::Sub,
                    ast::BinaryOperator::Mul => hir::BinaryOperator::Mul,
                    ast::BinaryOperator::Div => hir::BinaryOperator::Div,
                    ast::BinaryOperator::And => hir::BinaryOperator::And,
                    ast::BinaryOperator::Or => hir::BinaryOperator::Or,
                    ast::BinaryOperator::Mod => hir::BinaryOperator::Mod,
                    ast::BinaryOperator::BitAnd => hir::BinaryOperator::BitAnd,
                    ast::BinaryOperator::BitOr => hir::BinaryOperator::BitOr,
                    ast::BinaryOperator::BitXor => hir::BinaryOperator::BitXor,
                    ast::BinaryOperator::Shl => hir::BinaryOperator::Shl,
                    ast::BinaryOperator::Shr => hir::BinaryOperator::Shr,
                    ast::BinaryOperator::InstanceOf => hir::BinaryOperator::InstanceOf,
                },
                right: Box::new(Self::from_ast(right)),
                span: span.clone(),
            },
            ast::Expression::UnaryOperation {
                operator,
                expr,
                span,
            } => Expression::UnaryOperation {
                operator: match operator {
                    ast::UnaryOperator::Not => hir::UnaryOperator::Not,
                    ast::UnaryOperator::Neg => hir::UnaryOperator::Neg,
                },
                expr: Box::new(Self::from_ast(expr)),
                span: span.clone(),
            },
            ast::Expression::Paren(expr, span) => {
                Expression::Paren(Box::new(Self::from_ast(expr)), span.clone())
            }
        }
    }
}

impl Class {
    /// Lower a class from AST to HIR.
    pub fn from_ast(class: &ast::TypeExpressionBlock) -> Self {
        Class {
            name: class.name().to_string(),
            fields: class
                .fields
                .iter()
                .map(|field| Field {
                    name: field.name().to_string(),
                    r#type: field
                        .expr
                        .as_ref()
                        .map(type_ir_from_ast)
                        .unwrap_or_else(|| {
                            TypeIR::Primitive(
                                baml_types::TypeValue::String,
                                type_meta::IR {
                                    constraints: Vec::new(),
                                    streaming_behavior: StreamingBehavior::default(),
                                },
                            )
                        }),
                    span: field.span().clone(),
                })
                .collect(),
            methods: class.methods.iter().map(ExprFunction::from_ast).collect(),
            span: class.span().clone(),
        }
    }
}

impl Enum {
    /// Lower an enum from AST to HIR.
    pub fn from_ast(enum_def: &ast::TypeExpressionBlock) -> Self {
        Enum {
            name: enum_def.name().to_string(),
            variants: enum_def
                .fields
                .iter()
                .map(|field| EnumVariant {
                    name: field.name().to_string(),
                    span: field.span().clone(),
                })
                .collect(),
            span: enum_def.span().clone(),
        }
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    /// Test helper to generate HIR from BAML source
    fn pretty_hir_from_source(source: &'static str) -> String {
        let parser_db = crate::test::ast(source).unwrap_or_else(|e| panic!("{}", e));
        let hir = Hir::from_ast(&parser_db.ast);
        hir.pretty_print()
    }

    // Note: LLM function test disabled due to string literal parsing issues
    // TODO: Re-enable and fix string literal issues
    #[test]
    fn test_pretty_print_demo() {
        let source = r#"
          function fibonacci(n: int) -> int {
              let a = 0;
              let b = 1;
              let result = add(a, b);
              result
          }

          function add(x: int, y: int) -> int {
              x
          }
      "#;
        let ast = crate::test::ast(source).unwrap().ast;
        let hir = Hir::from_ast(&ast);
        println!("\n=== HIR Pretty Print Demo ===");
        println!("Original HIR structure:");
        println!("{}", hir.pretty_print());
        println!("\n=== With different line widths ===");
        println!("Line width 40:");
        println!("{}", hir.pretty_print_with_options(40, 2));
        println!("\nLine width 120:");
        println!("{}", hir.pretty_print_with_options(120, 2));
    }

    #[test]
    #[ignore] // This is about to change.
    fn test_class_constructor_with_complex_expressions() {
        // Test class constructor with both if expressions and expression blocks
        let source = r#"
          class Foo {
              a int
              b int
          }

          function TestConstructor() -> Foo {
              Foo { a: if true { 1 } else { 0 }, b: { let y = 1; y } }
          }
      "#;
        let result = pretty_hir_from_source(source);
        // The if expression in field 'a' should get lifted to temporary variables
        // The expression block in field 'b' should work correctly.
        let expected = r#"function TestConstructor() {
return Foo { a: if true { 1 } else { 0 }, b: {
let y = 1;
  y
} };
}

class Foo {
a: int
b: int
}"#;
        assert_eq!(result, expected);
        // Print for visual inspection
        println!("HIR for class constructor with complex expressions:");
        println!("{result}");
    }
}
