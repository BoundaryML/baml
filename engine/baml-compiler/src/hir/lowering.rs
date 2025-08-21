//! AST lowering to HIR.
//!
//! This files contains the convertions between Baml AST nodes to HIR nodes.

use baml_types::{type_meta::base::StreamingBehavior, Constraint, ConstraintLevel, TypeValue};
use internal_baml_ast::ast::{self, App, AssertStmt, Attribute, ReturnStmt, WithName, WithSpan};
use internal_baml_diagnostics::Span;

use crate::hir::{
    self, Block, Class, ClassConstructor, ClassConstructorField, Enum, EnumVariant, ExprFunction,
    Expression, Field, Hir, LlmFunction, Parameter, Statement, TypeArg, TypeM, TypeMeta,
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
                    hir.global_assignments
                        .insert(assignment.stmt.identifier.to_string(), value);
                }
                _ => {}
            }
        }

        hir
    }
}

impl TypeM<TypeMeta> {
    pub fn from_ast_optional(r#type: Option<&ast::FieldType>) -> Self {
        match r#type {
            Some(r#type) => Self::from_ast(r#type),
            None => Self::Null(TypeMeta {
                span: Span::fake(),
                constraints: Vec::new(),
                streaming_behavior: StreamingBehavior::default(),
            }),
        }
    }

    pub fn from_ast(type_: &ast::FieldType) -> Self {
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
                let arguments: Vec<&ast::Expression> = attr.arguments.arguments
                    .iter()
                    .map(|arg| &arg.value)
                    .collect();

                let (label, expression) = match arguments.as_slice() {
                    // Single argument: just the expression
                    [ast::Expression::JinjaExpressionValue(jinja_expr, _)] => {
                        (None, Some(jinja_expr.clone()))
                    }
                    // Two arguments: label and expression
                    [ast::Expression::Identifier(label_id), ast::Expression::JinjaExpressionValue(jinja_expr, _)] => {
                        (Some(label_id.to_string()), Some(jinja_expr.clone()))
                    }
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

        let meta = TypeMeta {
            span: type_.span().clone(),
            constraints,
            streaming_behavior,
        };

        match type_ {
            ast::FieldType::Symbol(_, name, _) => {
                if name.name().starts_with("Enum") {
                    TypeM::Enum(name.name().to_string(), meta)
                } else {
                    TypeM::Class(name.name().to_string(), meta)
                }
            }
            ast::FieldType::Primitive(_, prim, _, _) => match prim {
                TypeValue::Int => TypeM::Int(meta),
                TypeValue::String => TypeM::String(meta),
                TypeValue::Bool => TypeM::Bool(meta),
                TypeValue::Float => TypeM::String(meta), // TODO: Add Float type to TypeM
                TypeValue::Null => TypeM::String(meta),  // TODO: Add Null type to TypeM
                TypeValue::Media(_) => TypeM::String(meta), // TODO: Add Media type to TypeM
            },
            ast::FieldType::List(_, inner, dims, _, _) => {
                // Respect multi-dimensional arrays (e.g., int[][] has dims=2)
                let mut lowered_inner = Self::from_ast(inner);
                for _ in 0..*dims {
                    lowered_inner = TypeM::Array(Box::new(lowered_inner), meta.clone());
                }
                lowered_inner
            }
            ast::FieldType::Map(_, box_pair, _, _) => TypeM::Map(
                Box::new(Self::from_ast(&box_pair.0)),
                Box::new(Self::from_ast(&box_pair.1)),
                meta,
            ),
            ast::FieldType::Union(_, types, _, _) => {
                TypeM::Union(types.iter().map(Self::from_ast).collect(), meta)
            }
            _ => TypeM::String(meta), // Default case for other variants
        }
    }
    pub fn get_meta(&self) -> &TypeMeta {
        match self {
            TypeM::Int(meta) => meta,
            TypeM::String(meta) => meta,
            TypeM::Float(meta) => meta,
            TypeM::Bool(meta) => meta,
            TypeM::Null(meta) => meta,
            TypeM::Array(_, meta) => meta,
            TypeM::Map(_, _, meta) => meta,
            TypeM::Class(_, meta) => meta,
            TypeM::Enum(_, meta) => meta,
            TypeM::Union(_, meta) => meta,
            TypeM::Function(_, meta) => meta,
        }
    }

    /// Is the type complex enough that it should be parenthesized if it's not
    /// top-level?
    pub fn complex(&self) -> bool {
        let meta = self.get_meta();
        if meta.streaming_behavior != StreamingBehavior::default() {
            return true;
        }
        if !meta.constraints.is_empty() {
            return true;
        }
        match self {
            TypeM::Union(_, _) => true,
            TypeM::Int(_) => false,
            TypeM::Float(_) => false,
            TypeM::String(_) => false,
            TypeM::Bool(_) => false,
            TypeM::Array(_, _) => false,
            TypeM::Map(_, _, _) => false,
            TypeM::Class(_, _) => false,
            TypeM::Enum(_, _) => false,
            TypeM::Null(_) => false,
            TypeM::Function(_, _) => true,
        }
    }
}

impl LlmFunction {
    pub fn from_ast(function: &ast::ValueExprBlock) -> Self {
        LlmFunction {
            name: function.name().to_string(),
            parameters: function.input().map(lower_fn_args).unwrap_or_default(),

            return_type: TypeM::from_ast_optional(
                function.output().map(|output| &output.field_type),
            ),
            // return_type: TypeM::from_ast(function.output().unwrap_or(&FieldType::Primitive(
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
            is_mutable: param.is_mutable,
            r#type: TypeM::from_ast(&param.field_type),
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
            return_type: TypeM::from_ast_optional(function.return_type.as_ref()),
            body: Block::from_expr_block(&function.body),
            span: function.span.clone(),
        }
    }
}

impl Block {
    /// Lower an expression block into HIR for expression blocks.
    pub fn from_expr_block(block: &ast::ExpressionBlock) -> Self {
        Block {
            statements: block.stmts.iter().map(lower_stmt).collect(),
            trailing_expr: block
                .expr
                .as_deref()
                .map(Expression::from_ast)
                .map(Box::new),
        }
    }
}

fn lower_stmt(stmt: &ast::Stmt) -> Statement {
    match stmt {
        ast::Stmt::CForLoop(stmt) => {
            // we'll add  a block if we an init statement, otherwise we'll just
            // use the current context to push the while statement.

            let condition = stmt.condition.as_ref().map(Expression::from_ast);
            let init = stmt.init_stmt.as_ref().map(|b| lower_stmt(b));
            let block = Block::from_expr_block(&stmt.body);
            let after = stmt
                .after_stmt
                .as_ref()
                .map(|b| lower_stmt(b))
                .map(Box::new);

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

            match init {
                Some(init) => {
                    // use a block
                    Statement::Expression {
                        expr: Expression::Block(
                            Block {
                                statements: vec![init, inner_loop],
                                trailing_expr: None,
                            },
                            stmt.span.clone(),
                        ),
                        span: stmt.span.clone(),
                    }
                }
                // just inner loop
                None => inner_loop,
            }
        }
        ast::Stmt::Break(span) => Statement::Break(span.clone()),
        ast::Stmt::Continue(span) => Statement::Continue(span.clone()),
        ast::Stmt::WhileLoop(ast::WhileStmt {
            condition,
            body,
            span,
        }) => {
            // lowering to HIR is trivial, since HIR maps 1:1 with this.

            let condition = Expression::from_ast(condition);

            let body = Block::from_expr_block(body);

            Statement::While {
                condition,
                block: body,
                span: span.clone(),
            }
        }
        ast::Stmt::Assign(ast::AssignStmt { left, expr, span }) => Statement::Assign {
            left: Expression::from_ast(left),
            value: Expression::from_ast(expr),
            span: span.clone(),
        },
        ast::Stmt::AssignOp(ast::AssignOpStmt {
            left,
            assign_op,
            expr,
            span,
        }) => Statement::AssignOp {
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
        },
        ast::Stmt::Let(ast::LetStmt {
            identifier,
            is_mutable,
            expr,
            span,
        }) => {
            let lifted_expr = Expression::from_ast(expr);

            if *is_mutable {
                Statement::DeclareAndAssign {
                    name: identifier.to_string(),
                    value: lifted_expr,
                    span: span.clone(),
                }
            } else {
                Statement::Let {
                    name: identifier.to_string(),
                    value: lifted_expr,
                    span: span.clone(),
                }
            }
        }
        ast::Stmt::ForLoop(ast::ForLoopStmt {
            identifier,
            iterator,
            body,
            span,
        }) => {
            // Lower for loop to HIR
            let lifted_iterator = Expression::from_ast(iterator);

            // Add the for loop statement
            Statement::ForLoop {
                identifier: identifier.name().to_string(),
                iterator: Box::new(lifted_iterator),
                block: Block::from_expr_block(body),
                span: span.clone(),
            }
        }
        ast::Stmt::Expression(expr) => Statement::Expression {
            expr: Expression::from_ast(expr),
            span: expr.span().clone(),
        },
        ast::Stmt::Semicolon(expr) => Statement::Semicolon {
            expr: Expression::from_ast(expr),
            span: expr.span().clone(),
        },
        ast::Stmt::Return(ReturnStmt { value, span }) => Statement::Return {
            expr: Expression::from_ast(value),
            span: span.clone(),
        },
        ast::Stmt::Assert(AssertStmt { value, span }) => Statement::Assert {
            condition: Expression::from_ast(value),
            span: span.clone(),
        },
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
            ast::Expression::FieldAccess(base, field, span) => Expression::FieldAccess {
                base: Box::new(Self::from_ast(base)),
                field: field.to_string(),
                span: span.clone(),
            },
            ast::Expression::MethodCall {
                receiver,
                method,
                args,
                span,
            } => Expression::MethodCall {
                receiver: Box::new(Self::from_ast(receiver)),
                method: method.to_string(),
                args: args.iter().map(Self::from_ast).collect(),
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
                ..
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
                    .map(|arg| TypeArg::Type(TypeM::from_ast(arg)))
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
                    r#type: field.expr.as_ref().map(TypeM::from_ast).unwrap_or_else(|| {
                        TypeM::String(TypeMeta {
                            span: field.span().clone(),
                            constraints: Vec::new(),
                            streaming_behavior: StreamingBehavior::default(),
                        })
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
    fn hir_from_source(source: &'static str) -> String {
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

          fn add(x: int, y: int) -> int {
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
        let result = hir_from_source(source);
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
