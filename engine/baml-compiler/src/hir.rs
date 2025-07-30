use internal_baml_core::ast::{self, App, WithName, WithSpan};
use internal_baml_diagnostics::Span;

/// High-level intermediate representation.
///
/// This is analogous to the HIR in Rust: https://rustc-dev-guide.rust-lang.org/hir.html
/// It carries just enough information to produce BAML bytecode. It differs
/// from baml-core IR in that it does not contain any type information. It has limited
/// metadata, for use in debugging, namely source spans.
///
/// See `HIR::from_ast` to see how BAML syntax is lowered into HIR.
///
/// Lowering from AST to HIR involves desugaring certain syntax forms.
///   - For loops become while loops.
///   - Class constructor spreads become regular class constructors with
///     exhaustive fields.
///   - Implicit returns become explicit.
///   - If expressions become if statements with a block.
#[derive(Debug)]
pub struct Program {
    pub expr_functions: Vec<ExprFunction>,
    pub llm_functions: Vec<LLMFunction>,
    pub classes: Vec<Class>,
    pub enums: Vec<Enum>,
}

impl Program {
    /// Lower BAML AST into HIR.
    pub fn from_ast(ast: &ast::Ast) -> Self {
        let llm_functions = ast
            .iter_tops()
            .filter_map(|(_id, top)| match top {
                ast::Top::Function(function) => Some(LLMFunction::from_ast(function)),
                _ => None,
            })
            .collect();

        let expr_functions = ast
            .iter_tops()
            .filter_map(|(_id, top)| match top {
                ast::Top::ExprFn(expr_fn) => Some(ExprFunction::from_ast(expr_fn)),
                _ => None,
            })
            .collect();

        let classes = ast
            .iter_tops()
            .filter_map(|(_id, top)| match top {
                ast::Top::Class(class) => Some(Class::from_ast(class)),
                _ => None,
            })
            .collect();

        let enums = ast
            .iter_tops()
            .filter_map(|(_id, top)| match top {
                ast::Top::Enum(enum_def) => Some(Enum::from_ast(enum_def)),
                _ => None,
            })
            .collect();

        let hir = Program {
            expr_functions,
            llm_functions,
            classes,
            enums,
        };

        hir
    }
}

#[derive(Debug)]
pub struct ExprFunction {
    pub name: String,
    pub parameters: Vec<Parameter>,
    // pub return_type: Type,
    pub body: Block,
    pub span: Span,
}

#[derive(Debug)]
pub struct LLMFunction {
    pub name: String,
    pub parameters: Vec<Parameter>,
    // pub return_type: Type,
    pub client: String,
    pub prompt: String,
    pub span: Span,
}

#[derive(Debug)]
pub struct Class {
    pub name: String,
    pub fields: Vec<Field>,
    pub span: Span,
}

#[derive(Debug)]
pub struct Field {
    pub name: String,
    // pub r#type: Type,
    pub span: Span,
}

#[derive(Debug)]
pub struct Enum {
    pub name: String,
    pub variants: Vec<EnumVariant>,
    pub span: Span,
}

#[derive(Debug)]
pub struct EnumVariant {
    pub name: String,
    pub span: Span,
}

#[derive(Debug)]
pub struct Parameter {
    pub name: String,
    // pub r#type: Type,
    pub span: Span,
}

#[derive(Debug)]
pub struct Block {
    pub statements: Vec<Statement>,
}

/// A single unit of execution within a block.
#[derive(Debug)]
pub enum Statement {
    /// Assign an immutable variable.
    Let {
        name: String,
        value: Expression,
        span: Span,
    },
    /// Declare a (mutable) reference.
    /// There is no span because it is never present in the source AST.
    /// This is a desugaring from `if` expressions.
    DeclareReference { name: String, span: Span },
    /// Assign a mutable variable.
    Assign { name: String, value: Expression },
    /// Declare and assign a mutable reference in one statement.
    DeclareAndAssign {
        name: String,
        value: Expression,
        span: Span,
    },
    /// Return from a function.
    Return { expr: Expression, span: Span },
    /// Evaluate an expression as the final value of a block (without returning from function).
    Expression { expr: Expression, span: Span },
    If {
        condition: Box<Expression>,
        then_block: Block,
        else_block: Option<Block>,
        span: Span,
    },
    While {
        condition: Box<Expression>,
        block: Block,
        span: Span,
    },
    ForLoop {
        identifier: String,
        iterator: Box<Expression>,
        block: Block,
        span: Span,
    },
}

/// Expressions
#[derive(Debug)]
pub enum Expression {
    BoolValue(bool, Span),
    NumericValue(String, Span),
    Identifier(String, Span),
    StringValue(String, Span),
    RawStringValue(String, Span),
    Array(Vec<Expression>, Span),
    Map(Vec<(Expression, Expression)>, Span),
    JinjaExpressionValue(String, Span),
    Call(String, Vec<Expression>, Span),
    // Lambda(ArgumentsList, Box<ExpressionBlock>, Span), // TODO.
    // MethodCall(Box<Expression>, String, Vec<Expression>), // TODO.
    ClassConstructor(ClassConstructor, Span),
    /// Expression block - has its own scope with statements and evaluates to a value
    ExpressionBlock(Box<Block>, Span),
    /// If expression - evaluates condition and returns value from one branch
    If(Box<Expression>, Box<Expression>, Option<Box<Expression>>, Span),
}

#[derive(Debug)]
pub struct ClassConstructor {
    pub class_name: String,
    pub fields: Vec<ClassConstructorField>,
}

#[derive(Debug)]
pub enum ClassConstructorField {
    Named { name: String, value: Expression },
    Spread { value: Expression },
}

impl LLMFunction {
    pub fn from_ast(function: &ast::ValueExprBlock) -> Self {
        LLMFunction {
            name: function.name().to_string(),
            parameters: function
                .input()
                .map(|input| {
                    input
                        .args
                        .iter()
                        .map(|(name, _)| Parameter {
                            name: name.to_string(),
                            // r#type: param.r#type.to_string(),
                            span: name.span().clone(),
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or(vec![]),
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

impl ExprFunction {
    /// Lower an expression function into HIR.
    pub fn from_ast(function: &ast::ExprFn) -> Self {
        ExprFunction {
            name: function.name.to_string(),
            parameters: function
                .args
                .args
                .iter()
                .map(|(name, _)| Parameter {
                    name: name.to_string(),
                    // r#type: param.r#type.to_string(),
                    span: name.span().clone(),
                })
                .collect::<Vec<_>>(),
            body: Block::from_function_body(&function.body),
            span: function.span.clone(),
        }
    }
}

impl Block {
    /// Lower an expression block into HIR for function bodies (ends with Statement::Return).
    pub fn from_function_body(block: &ast::ExpressionBlock) -> Self {
        Self::from_ast_with_context(block, true)
    }

    /// Lower an expression block into HIR for expression blocks (ends with Statement::Expression).
    pub fn from_expression_block(block: &ast::ExpressionBlock) -> Self {
        Self::from_ast_with_context(block, false)
    }

    /// Lower an expression block into HIR with specified context.
    /// If is_function_body is true, the final expression becomes Statement::Return.
    /// If is_function_body is false, the final expression becomes Statement::Expression.
    fn from_ast_with_context(block: &ast::ExpressionBlock, is_function_body: bool) -> Self {
        let mut statements = vec![];

        // Process statements
        for stmt in &block.stmts {
            match stmt {
                ast::Stmt::Let(ast::LetStmt {
                    identifier,
                    expr,
                    span,
                }) => {
                    // Process all let statements uniformly
                    let mut temp_counter = 0;
                    let mut lifted_statements = vec![];
                    let lifted_expr = Expression::from_ast(
                        expr,
                        true,
                        &mut lifted_statements,
                        &mut temp_counter,
                    );

                    // Add any lifted statements first
                    statements.extend(lifted_statements);

                    // Then add the actual let statement
                    statements.push(Statement::Let {
                        name: identifier.to_string(),
                        value: lifted_expr,
                        span: span.clone(),
                    });
                }
                ast::Stmt::ForLoop(ast::ForLoopStmt {
                    identifier,
                    iterator,
                    body,
                    span,
                }) => {
                    // Lower for loop to HIR
                    let mut temp_counter = 0;
                    let mut lifted_statements = vec![];
                    let lifted_iterator = Expression::from_ast(
                        iterator,
                        true,
                        &mut lifted_statements,
                        &mut temp_counter,
                    );

                    // Add any lifted statements first
                    statements.extend(lifted_statements);

                    // Add the for loop statement
                    statements.push(Statement::ForLoop {
                        identifier: identifier.name().to_string(),
                        iterator: Box::new(lifted_iterator),
                        block: Block::from_expression_block(body),
                        span: span.clone(),
                    });
                }
            }
        }

        // Process the final expression with lifting to handle if expressions
        let mut temp_counter = 0;
        let mut lifted_statements = vec![];
        let lifted_expr = Expression::from_ast(
            block.expr.as_ref(),
            true,
            &mut lifted_statements,
            &mut temp_counter,
        );

        // Add any lifted statements first
        statements.extend(lifted_statements);

        // Then add the final statement
        statements.push(if is_function_body {
            Statement::Return {
                expr: lifted_expr,
                span: block.expr.span().clone(),
            }
        } else {
            Statement::Expression {
                expr: lifted_expr,
                span: block.expr.span().clone(),
            }
        });

        Block { statements }
    }

}


impl Expression {
    /// Lower an expression into HIR.
    ///
    /// If `with_lifting` is true, if expressions will be lifted to temporary variables
    /// and the statements will be added to the provided vector.
    /// If `with_lifting` is false, if expressions will fall back to placeholders.
    pub fn from_ast(
        expr: &ast::Expression,
        with_lifting: bool,
        statements: &mut Vec<Statement>,
        temp_counter: &mut usize,
    ) -> Self {
        match expr {
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
            ast::Expression::Array(values, span) => Expression::Array(
                values
                    .iter()
                    .map(|value| Self::from_ast(value, with_lifting, statements, temp_counter))
                    .collect(),
                span.clone(),
            ),
            ast::Expression::App(App {
                name, args, span, ..
            }) => Expression::Call(
                name.to_string(),
                args.iter()
                    .map(|arg| Self::from_ast(arg, with_lifting, statements, temp_counter))
                    .collect(),
                span.clone(),
            ),
            ast::Expression::Map(pairs, span) => Expression::Map(
                pairs
                    .iter()
                    .map(|(key, value)| {
                        (
                            Self::from_ast(key, with_lifting, statements, temp_counter),
                            Self::from_ast(value, with_lifting, statements, temp_counter),
                        )
                    })
                    .collect(),
                span.clone(),
            ),
            ast::Expression::If(condition, then_expr, else_expr, span) => {
                match else_expr {
                    Some(else_expr) => {
                        // If expression with else branch - preserve as expression
                        Expression::If(
                            Box::new(Self::from_ast(
                                condition,
                                false,  // Don't lift condition - it's always evaluated
                                statements,
                                temp_counter,
                            )),
                            Box::new(Self::from_ast(
                                then_expr,
                                false,  // Don't lift branches - only one is evaluated
                                statements,
                                temp_counter,
                            )),
                            Some(Box::new(Self::from_ast(
                                else_expr,
                                false,  // Don't lift branches - only one is evaluated
                                statements,
                                temp_counter,
                            ))),
                            span.clone(),
                        )
                    }
                    None => {
                        // If without else can't produce a reliable value
                        // This should be caught by validation
                        panic!("if expression without else branch cannot be used as a value");
                    }
                }
            }
            ast::Expression::ExprBlock(block, span) => {
                // Expression blocks are lowered to HIR preserving their structure
                // This maintains proper scoping - variables defined inside the block
                // are only visible within that block
                Expression::ExpressionBlock(
                    Box::new(Block::from_expression_block(block)),
                    span.clone(),
                )
            }
            ast::Expression::Lambda(_args, _body, span) => {
                // Lambdas are not yet implemented
                Expression::StringValue("lambda_todo".to_string(), span.clone())
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
                            .map(|field| {
                                match field {
                                    ast::ClassConstructorField::Named(name, expr) => {
                                        ClassConstructorField::Named {
                                            name: name.to_string(),
                                            value: Self::from_ast(
                                                expr,
                                                with_lifting,
                                                statements,
                                                temp_counter,
                                            ),
                                        }
                                    }
                                    ast::ClassConstructorField::Spread(expr) => {
                                        ClassConstructorField::Spread {
                                            value: Self::from_ast(
                                                expr,
                                                with_lifting,
                                                statements,
                                                temp_counter,
                                            ),
                                        }
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
                    span: field.span().clone(),
                })
                .collect(),
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
    use internal_baml_core::ast;
    use internal_baml_diagnostics::SourceFile;

    /// Test helper to generate HIR from BAML source
    fn hir_from_source(source: &str) -> Program {
        let ast = parse_baml(source);
        Program::from_ast(&ast)
    }

    /// Parse BAML source code and return the AST
    fn parse_baml(source: &str) -> ast::Ast {
        let path = std::path::PathBuf::from("test.baml");
        let source_file = SourceFile::from((path.clone(), source));

        let validated_schema = internal_baml_core::validate(&path, vec![source_file]);

        if validated_schema.diagnostics.has_errors() {
            panic!(
                "Parse errors: {}",
                validated_schema.diagnostics.to_pretty_string()
            );
        }

        validated_schema.db.ast
    }

    // Test cases start here

    #[test]
    fn test_simple_expression_function() {
        let source = r#"
            fn MyFunc(x: int, y: string) -> int {
                42
            }
        "#;

        let hir = hir_from_source(source);
        assert_eq!(hir.expr_functions.len(), 1);
        assert_eq!(hir.expr_functions[0].name, "MyFunc");
        assert_eq!(hir.expr_functions[0].parameters.len(), 2);
    }

    #[test]
    fn test_expression_with_let_binding() {
        let source = r#"
            fn AddOne(x: int) -> int {
                let y = x;
                y
            }
        "#;

        let hir = hir_from_source(source);
        assert_eq!(hir.expr_functions.len(), 1);
        assert_eq!(hir.expr_functions[0].body.statements.len(), 2);
    }

    #[test]
    fn test_if_expression_desugaring() {
        // Test if expression preservation in let bindings
        let source = r#"
            fn simpleIf() -> string {
                let x = if true { "yes" } else { "no" };
                x
            }
        "#;

        let hir = hir_from_source(source);
        assert_eq!(hir.expr_functions.len(), 1);
        // Should have 2 statements: let x = if..., and return x
        assert_eq!(hir.expr_functions[0].body.statements.len(), 2);
    }

    #[test]
    fn test_class_lowering() {
        let source = r#"
            class Point {
                x int
                y int
            }
        "#;

        let hir = hir_from_source(source);
        assert_eq!(hir.classes.len(), 1);
        assert_eq!(hir.classes[0].name, "Point");
        assert_eq!(hir.classes[0].fields.len(), 2);
    }

    #[test]
    fn test_enum_lowering() {
        let source = r#"
            enum Color {
                Red
                Green
                Blue
            }
        "#;

        let hir = hir_from_source(source);
        assert_eq!(hir.enums.len(), 1);
        assert_eq!(hir.enums[0].name, "Color");
        assert_eq!(hir.enums[0].variants.len(), 3);
    }
}
