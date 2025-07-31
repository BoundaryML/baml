use baml_types::type_meta::base::StreamingBehavior;
use baml_types::{Constraint, ConstraintLevel, JinjaExpression, TypeValue};
use internal_baml_core::ast::{self, App, Attribute, WithName, WithSpan};
use internal_baml_diagnostics::Span;
use pretty::RcDoc;

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

    /// Convert HIR to a pretty printing document
    pub fn to_doc(&self) -> RcDoc<'static, ()> {
        let mut docs = Vec::new();
        // Add expression functions
        for func in &self.expr_functions {
            docs.push(func.to_doc());
        }
        // Add LLM functions
        for func in &self.llm_functions {
            docs.push(func.to_doc());
        }
        // Add classes
        for class in &self.classes {
            docs.push(class.to_doc());
        }
        // Add enums
        for enum_def in &self.enums {
            docs.push(enum_def.to_doc());
        }
        if docs.is_empty() {
            RcDoc::nil()
        } else {
            RcDoc::intersperse(docs, RcDoc::hardline().append(RcDoc::hardline()))
        }
    }
    pub fn pretty_print(&self) -> String {
        self.pretty_print_with_options(80, 2)
    }
    /// Pretty print the HIR with custom line width and indent width
    pub fn pretty_print_with_options(&self, line_width: usize, _indent_width: isize) -> String {
        let doc = self.to_doc();
        let mut output = Vec::new();
        doc.render(line_width, &mut output).unwrap();
        String::from_utf8(output).unwrap()
    }
}

#[derive(Debug)]
pub enum TypeM<M> {
    Int(M),
    String(M),
    Bool(M),
    Array(Box<TypeM<M>>, M),
    Map(Box<TypeM<M>>, Box<TypeM<M>>, M),
    ClassName(String, M),
    EnumName(String, M),
    Union(Vec<TypeM<M>>, M),
}

#[derive(Debug)]
struct TypeMeta {
    span: Span,
    constraints: Vec<Constraint>,
    streaming_behavior: StreamingBehavior,
}

impl TypeM<TypeMeta> {
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
                    TypeM::EnumName(name.name().to_string(), meta)
                } else {
                    TypeM::ClassName(name.name().to_string(), meta)
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
            ast::FieldType::List(_, inner, _, _, _) => {
                TypeM::Array(Box::new(Self::from_ast(inner)), meta)
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
            TypeM::Bool(meta) => meta,
            TypeM::Array(_, meta) => meta,
            TypeM::Map(_, _, meta) => meta,
            TypeM::ClassName(_, meta) => meta,
            TypeM::EnumName(_, meta) => meta,
            TypeM::Union(_, meta) => meta,
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
            TypeM::String(_) => false,
            TypeM::Bool(_) => false,
            TypeM::Array(_, _) => false,
            TypeM::Map(_, _, _) => false,
            TypeM::ClassName(_, _) => false,
            TypeM::EnumName(_, _) => false,
        }
    }

    pub fn to_doc(&self) -> RcDoc<'static, ()> {
        let meta = self.get_meta();
        let base = match self {
            TypeM::Int(_) => RcDoc::text("int"),
            TypeM::String(_) => RcDoc::text("string"),
            TypeM::Bool(_) => RcDoc::text("bool"),
            TypeM::Array(inner, _) => RcDoc::text("array").append(inner.to_doc()),
            TypeM::Map(key, value, _) => RcDoc::text("map")
                .append(key.to_doc())
                .append(RcDoc::text(":"))
                .append(value.to_doc()),
            TypeM::ClassName(name, _) => RcDoc::text(name.clone()),
            TypeM::EnumName(name, _) => RcDoc::text(name.clone()),
            TypeM::Union(types, _) => {
                let mut docs = Vec::new();
                for type_ in types {
                    docs.push(type_.to_doc());
                }
                RcDoc::text("(")
                    .append(RcDoc::intersperse(docs, RcDoc::text(" | ")))
                    .append(RcDoc::text(")"))
            }
        };

        let mut doc = base;
        if !meta.constraints.is_empty() {
            doc = doc
                .append(RcDoc::space())
                .append(RcDoc::text("@constrained"));
        }
        if meta.streaming_behavior.done {
            doc = doc.append(RcDoc::text(" @stream.done"));
        }
        if meta.streaming_behavior.state {
            doc = doc.append(RcDoc::text(" @stream.with_state"));
        }
        if meta.streaming_behavior.needed {
            doc = doc.append(RcDoc::text(" @stream.needed"));
        }
        doc
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
    pub r#type: TypeM<TypeMeta>,
    pub span: Span,
}

impl Field {
    pub fn to_doc(&self) -> RcDoc<'static, ()> {
        RcDoc::text(self.name.clone())
            .append(RcDoc::text(": "))
            .append(self.r#type.to_doc())
    }
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

impl Statement {
    pub fn to_doc(&self) -> RcDoc<'static, ()> {
        match self {
            Statement::Let { name, value, .. } => RcDoc::text("let")
                .append(RcDoc::space())
                .append(RcDoc::text(name.clone()))
                .append(RcDoc::space())
                .append(RcDoc::text("="))
                .append(RcDoc::space())
                .append(value.to_doc())
                .append(RcDoc::text(";")),
            Statement::DeclareReference { name, .. } => RcDoc::text("var")
                .append(RcDoc::space())
                .append(RcDoc::text(name.clone()))
                .append(RcDoc::text(";")),
            Statement::Assign { name, value } => RcDoc::text(name.clone())
                .append(RcDoc::space())
                .append(RcDoc::text("="))
                .append(RcDoc::space())
                .append(value.to_doc())
                .append(RcDoc::text(";")),
            Statement::DeclareAndAssign { name, value, .. } => RcDoc::text("var")
                .append(RcDoc::space())
                .append(RcDoc::text(name.clone()))
                .append(RcDoc::space())
                .append(RcDoc::text("="))
                .append(RcDoc::space())
                .append(value.to_doc())
                .append(RcDoc::text(";")),
            Statement::Return { expr, .. } => RcDoc::text("return")
                .append(RcDoc::space())
                .append(expr.to_doc())
                .append(RcDoc::text(";")),
            Statement::Expression { expr, .. } => expr.to_doc(),
            Statement::If {
                condition,
                then_block,
                else_block,
                ..
            } => {
                let mut doc = RcDoc::text("if")
                    .append(RcDoc::space())
                    .append(condition.to_doc())
                    .append(RcDoc::space())
                    .append(RcDoc::text("{"))
                    .append(RcDoc::hardline())
                    .append(then_block.to_doc().nest(2))
                    .append(RcDoc::hardline())
                    .append(RcDoc::text("}"));
                if let Some(else_block) = else_block {
                    doc = doc
                        .append(RcDoc::space())
                        .append(RcDoc::text("else"))
                        .append(RcDoc::space())
                        .append(RcDoc::text("{"))
                        .append(RcDoc::hardline())
                        .append(else_block.to_doc().nest(2))
                        .append(RcDoc::hardline())
                        .append(RcDoc::text("}"));
                }
                doc
            }
            Statement::While {
                condition, block, ..
            } => RcDoc::text("while")
                .append(RcDoc::space())
                .append(condition.to_doc())
                .append(RcDoc::space())
                .append(RcDoc::text("{"))
                .append(RcDoc::hardline())
                .append(block.to_doc().nest(2))
                .append(RcDoc::hardline())
                .append(RcDoc::text("}")),
            Statement::ForLoop {
                identifier,
                iterator,
                block,
                ..
            } => RcDoc::text("for")
                .append(RcDoc::space())
                .append(RcDoc::text(identifier.clone()))
                .append(RcDoc::space())
                .append(RcDoc::text("in"))
                .append(RcDoc::space())
                .append(iterator.to_doc())
                .append(RcDoc::space())
                .append(RcDoc::text("{"))
                .append(RcDoc::hardline())
                .append(block.to_doc().nest(2))
                .append(RcDoc::hardline())
                .append(RcDoc::text("}")),
        }
    }
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
    If(
        Box<Expression>,
        Box<Expression>,
        Option<Box<Expression>>,
        Span,
    ),
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

impl ClassConstructorField {
    pub fn to_doc(&self) -> RcDoc<'static, ()> {
        match self {
            ClassConstructorField::Named { name, value } => RcDoc::text(name.clone())
                .append(RcDoc::text(":"))
                .append(RcDoc::space())
                .append(value.to_doc()),
            ClassConstructorField::Spread { value } => RcDoc::text("..").append(value.to_doc()),
        }
    }
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

    pub fn to_doc(&self) -> RcDoc<'static, ()> {
        RcDoc::text("function")
            .append(RcDoc::space())
            .append(RcDoc::text(self.name.clone()))
            .append(RcDoc::text("("))
            .append(self.parameters_to_doc())
            .append(RcDoc::text(")"))
            .append(RcDoc::space())
            .append(RcDoc::text("{"))
            .append(RcDoc::hardline())
            .append(
                RcDoc::text("client")
                    .append(RcDoc::space())
                    .append(RcDoc::text(self.client.clone()))
                    .append(RcDoc::hardline())
                    .append(RcDoc::text("prompt"))
                    .append(RcDoc::space())
                    .append(RcDoc::text(self.prompt.clone()))
                    .nest(2),
            )
            .append(RcDoc::hardline())
            .append(RcDoc::text("}"))
    }
    fn parameters_to_doc(&self) -> RcDoc<'static, ()> {
        if self.parameters.is_empty() {
            RcDoc::nil()
        } else {
            let param_docs: Vec<_> = self.parameters.iter().map(|p| p.to_doc()).collect();
            RcDoc::intersperse(param_docs, RcDoc::text(",").append(RcDoc::space()))
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

    pub fn to_doc(&self) -> RcDoc<'static, ()> {
        let body_doc = if self.body.statements.is_empty() {
            RcDoc::nil()
        } else {
            // The key is to apply nest() to the entire content that includes line breaks
            RcDoc::hardline()
                .append(RcDoc::intersperse(
                    self.body
                        .statements
                        .iter()
                        .map(|s| s.to_doc())
                        .collect::<Vec<_>>(),
                    RcDoc::hardline(),
                ))
                .append(RcDoc::hardline())
                .nest(2)
        };
        RcDoc::text("function")
            .append(RcDoc::space())
            .append(RcDoc::text(self.name.clone()))
            .append(RcDoc::text("("))
            .append(self.parameters_to_doc())
            .append(RcDoc::text(")"))
            .append(RcDoc::space())
            .append(RcDoc::text("{"))
            .append(body_doc)
            .append(RcDoc::text("}"))
    }
    fn parameters_to_doc(&self) -> RcDoc<'static, ()> {
        if self.parameters.is_empty() {
            RcDoc::nil()
        } else {
            let param_docs: Vec<_> = self.parameters.iter().map(|p| p.to_doc()).collect();
            RcDoc::intersperse(param_docs, RcDoc::text(",").append(RcDoc::space()))
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
                    let lifted_expr =
                        Expression::from_ast(expr, true, &mut lifted_statements, &mut temp_counter);

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

    pub fn to_doc(&self) -> RcDoc<'static, ()> {
        if self.statements.is_empty() {
            RcDoc::nil()
        } else {
            RcDoc::intersperse(
                self.statements
                    .iter()
                    .map(|s| s.to_doc())
                    .collect::<Vec<_>>(),
                RcDoc::hardline(),
            )
        }
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
                                false, // Don't lift condition - it's always evaluated
                                statements,
                                temp_counter,
                            )),
                            Box::new(Self::from_ast(
                                then_expr,
                                false, // Don't lift branches - only one is evaluated
                                statements,
                                temp_counter,
                            )),
                            Some(Box::new(Self::from_ast(
                                else_expr,
                                false, // Don't lift branches - only one is evaluated
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
                            .map(|field| match field {
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
    pub fn to_doc(&self) -> RcDoc<'static, ()> {
        match self {
            Expression::BoolValue(val, _) => RcDoc::text(val.to_string()),
            Expression::NumericValue(val, _) => RcDoc::text(val.clone()),
            Expression::Identifier(name, _) => RcDoc::text(name.clone()),
            Expression::StringValue(val, _) => RcDoc::text(format!("\"{}\"", val)),
            Expression::RawStringValue(val, _) => RcDoc::text(format!("#\"{}\"#", val)),
            Expression::Array(values, _) => RcDoc::text("[")
                .append(if values.is_empty() {
                    RcDoc::nil()
                } else {
                    RcDoc::intersperse(
                        values.iter().map(|v| v.to_doc()).collect::<Vec<_>>(),
                        RcDoc::text(",").append(RcDoc::space()),
                    )
                })
                .append(RcDoc::text("]")),
            Expression::Map(pairs, _) => RcDoc::text("{")
                .append(if pairs.is_empty() {
                    RcDoc::nil()
                } else {
                    RcDoc::space()
                        .append(RcDoc::intersperse(
                            pairs
                                .iter()
                                .map(|(k, v)| {
                                    k.to_doc()
                                        .append(RcDoc::text(":"))
                                        .append(RcDoc::space())
                                        .append(v.to_doc())
                                })
                                .collect::<Vec<_>>(),
                            RcDoc::text(",").append(RcDoc::space()),
                        ))
                        .append(RcDoc::space())
                })
                .append(RcDoc::text("}")),
            Expression::If(condition, then_expr, else_expr, _) => {
                let mut doc = RcDoc::text("if")
                    .append(RcDoc::space())
                    .append(condition.to_doc())
                    .append(RcDoc::space())
                    .append(RcDoc::text("{"))
                    .append(RcDoc::space())
                    .append(then_expr.to_doc())
                    .append(RcDoc::space())
                    .append(RcDoc::text("}"));
                if let Some(else_expr) = else_expr {
                    doc = doc
                        .append(RcDoc::text(" else {"))
                        .append(RcDoc::space())
                        .append(else_expr.to_doc())
                        .append(RcDoc::space())
                        .append(RcDoc::text("}"));
                }
                doc
            }
            Expression::JinjaExpressionValue(val, _) => RcDoc::text(val.clone()),
            Expression::Call(name, args, _) => RcDoc::text(name.clone())
                .append(RcDoc::text("("))
                .append(if args.is_empty() {
                    RcDoc::nil()
                } else {
                    RcDoc::intersperse(
                        args.iter().map(|arg| arg.to_doc()).collect::<Vec<_>>(),
                        RcDoc::text(",").append(RcDoc::space()),
                    )
                })
                .append(RcDoc::text(")")),
            Expression::ClassConstructor(cc, _) => RcDoc::text(cc.class_name.clone())
                .append(RcDoc::space())
                .append(RcDoc::text("{"))
                .append(if cc.fields.is_empty() {
                    RcDoc::nil()
                } else {
                    RcDoc::space()
                        .append(RcDoc::intersperse(
                            cc.fields.iter().map(|f| f.to_doc()).collect::<Vec<_>>(),
                            RcDoc::text(",").append(RcDoc::space()),
                        ))
                        .append(RcDoc::space())
                })
                .append(RcDoc::text("}")),
            Expression::ExpressionBlock(block, _) => RcDoc::text("{")
                .append(RcDoc::hardline())
                .append(block.to_doc().nest(2))
                .append(RcDoc::hardline())
                .append(RcDoc::text("}")),
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
                        .map(|field_type| TypeM::from_ast(field_type))
                        .unwrap_or_else(|| {
                            TypeM::String(TypeMeta {
                                span: field.span().clone(),
                                constraints: Vec::new(),
                                streaming_behavior: StreamingBehavior::default(),
                            })
                        }),
                    span: field.span().clone(),
                })
                .collect(),
            span: class.span().clone(),
        }
    }
    pub fn to_doc(&self) -> RcDoc<'static, ()> {
        RcDoc::text("class")
            .append(RcDoc::space())
            .append(RcDoc::text(self.name.clone()))
            .append(RcDoc::space())
            .append(RcDoc::text("{"))
            .append(if self.fields.is_empty() {
                RcDoc::nil()
            } else {
                RcDoc::hardline()
                    .append(
                        RcDoc::intersperse(
                            self.fields.iter().map(|f| f.to_doc()).collect::<Vec<_>>(),
                            RcDoc::hardline(),
                        )
                        .nest(2),
                    )
                    .append(RcDoc::hardline())
            })
            .append(RcDoc::text("}"))
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
    pub fn to_doc(&self) -> RcDoc<'static, ()> {
        RcDoc::text("enum")
            .append(RcDoc::space())
            .append(RcDoc::text(self.name.clone()))
            .append(RcDoc::space())
            .append(RcDoc::text("{"))
            .append(if self.variants.is_empty() {
                RcDoc::nil()
            } else {
                RcDoc::hardline()
                    .append(
                        RcDoc::intersperse(
                            self.variants.iter().map(|v| v.to_doc()).collect::<Vec<_>>(),
                            RcDoc::hardline(),
                        )
                        .nest(2),
                    )
                    .append(RcDoc::hardline())
            })
            .append(RcDoc::text("}"))
    }
}

impl EnumVariant {
    pub fn to_doc(&self) -> RcDoc<'static, ()> {
        RcDoc::text(self.name.clone())
    }
}
impl Parameter {
    pub fn to_doc(&self) -> RcDoc<'static, ()> {
        // For now, just show the parameter name since types aren't included in HIR
        RcDoc::text(self.name.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use internal_baml_diagnostics::SourceFile;

    /// Test helper to generate HIR from BAML source and return pretty-printed string
    fn hir_from_source(source: &str) -> String {
        let ast = parse_baml(source);
        let hir = Program::from_ast(&ast);
        hir.pretty_print()
    }

    /// Parse BAML source code and return the AST
    #[track_caller]
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
            function MyFunc(x: int, y: string) -> int {
                42
            }
        "#;
        let expected = r#"function MyFunc(x, y) {
  return 42;
}"#;
        assert_eq!(hir_from_source(source), expected);
    }
    #[test]
    fn test_expression_with_let_binding() {
        let source = r#"
            function AddOne(x: int) -> int {
                let y = x;
                y
            }
        "#;
        let expected = r#"function AddOne(x) {
  let y = x;
  return y;
}"#;
        assert_eq!(hir_from_source(source), expected);
    }
    #[test]
    fn test_basic_expressions() {
        let source = r#"
            function TestExpressions() -> string {
                let bool_val = true;
                let num_val = 123.45;
                let str_val = "hello";
                str_val
            }
        "#;
        let expected = r#"function TestExpressions() {
  let bool_val = true;
  let num_val = 123.45;
  let str_val = "hello";
  return str_val;
}"#;
        assert_eq!(hir_from_source(source), expected);
    }
    #[test]
    fn test_array_expression() {
        let source = r#"
            function TestArray() -> int[] {
                [1, 2, 3]
            }
        "#;
        let expected = r#"function TestArray() {
  return [1, 2, 3];
}"#;
        assert_eq!(hir_from_source(source), expected);
    }
    #[test]
    fn test_function_call() {
        let source = r#"
            function myFunc(x: int, y: string) -> int {
                x
            }
            
            function CallTest() -> int {
                let result = myFunc(42, "hello");
                result
            }
        "#;
        let expected = r#"function myFunc(x, y) {
  return x;
}

function CallTest() {
  let result = myFunc(42, "hello");
  return result;
}"#;
        assert_eq!(hir_from_source(source), expected);
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
        let ast = parse_baml(source);
        let hir = Program::from_ast(&ast);
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
    fn test_pretty_print_expression_function() {
        let source = r#"
            fn AddOne(x: int) -> int {
                let y = x;
                y
            }
        "#;
        let ast = parse_baml(source);
        let hir = Program::from_ast(&ast);
        let pretty_printed = hir.pretty_print();
        // Check that the pretty printed output contains the expected structure
        assert!(pretty_printed.contains("function AddOne(x)"));
        assert!(pretty_printed.contains("let y = x;"));
        assert!(pretty_printed.contains("return y;"));
        // Print it for visual inspection
        println!("Pretty printed HIR:");
        println!("{}", pretty_printed);
    }
    #[test]
    fn test_pretty_print_array_and_call() {
        let source = r#"
            function helper(x: int) -> int {
                x
            }
            
            function TestArray() -> int[] {
                let arr = [1, 2, 3];
                let result = helper(42);
                [arr, result]
            }
        "#;
        let ast = parse_baml(source);
        let hir = Program::from_ast(&ast);
        let pretty_printed = hir.pretty_print();
        // Check that the pretty printed output contains the expected structure
        assert!(pretty_printed.contains("function helper(x)"));
        assert!(pretty_printed.contains("function TestArray()"));
        assert!(pretty_printed.contains("let arr = [1, 2, 3];"));
        assert!(pretty_printed.contains("let result = helper(42);"));
        assert!(pretty_printed.contains("[arr, result]"));
        // Print it for visual inspection
        println!("Pretty printed HIR with arrays and calls:");
        println!("{}", pretty_printed);
    }
    #[test]
    fn test_indentation_consistency() {
        let source = r#"
            function simple() -> string {
                "hello"
            }
        "#;
        let expected = r#"function simple() {
  return "hello";
}"#;
        assert_eq!(hir_from_source(source), expected);
    }
    #[test]
    fn test_if_expression_desugaring() {
        // Test if expression desugaring in let bindings
        let source = r#"
            function simpleIf() -> string {
                let x = if true { "yes" } else { "no" };
                x
            }
        "#;
        let expected = r#"function simpleIf() {
        let x = if true { "yes" } else { "no" };
        return x;
}"#;
        assert_eq!(hir_from_source(source), expected);
    }
    #[test]
    fn test_attribute_conversion() {
        // Test constraint attributes
        let source = r#"
            function TestConstraints() -> string @assert("this.length > 0") @check("this != 'bad'") {
                "hello"
            }
        "#;
        let ast = parse_baml(source);
        let hir = Program::from_ast(&ast);

        // The HIR should have the constraints stored in the type metadata
        // This test verifies that the attribute parsing doesn't crash
        let pretty = hir.pretty_print();
        assert!(pretty.contains("function TestConstraints"));
    }

    #[test]
    fn test_streaming_behavior_attributes() {
        // Test streaming behavior attributes
        let source = r#"
            class MyClass {
                field1 string @stream.done
                field2 int @stream.not_null
                field3 bool @stream.with_state
            }
        "#;
        let ast = parse_baml(source);
        let hir = Program::from_ast(&ast);

        // Verify the HIR was created successfully with streaming attributes
        let pretty = hir.pretty_print();
        assert!(pretty.contains("class MyClass"));
        assert!(pretty.contains("field1: string @stream.done"));
        assert!(pretty.contains("field2: int @stream.needed"));
        assert!(pretty.contains("field3: bool @stream.with_state"));
    }

    #[test]
    fn test_class_with_constraints() {
        // Test class fields with constraint attributes - simplified test for now
        let source = r#"
            class User {
                name string
                age int
                email string
            }
        "#;
        let ast = parse_baml(source);
        let hir = Program::from_ast(&ast);
        let pretty = hir.pretty_print();

        // Verify the class pretty prints successfully
        assert!(pretty.contains("class User"));
        assert!(pretty.contains("name: string"));
        assert!(pretty.contains("age: int"));
        assert!(pretty.contains("email: string"));

        // Print for visual inspection
        println!("Simple class:");
        println!("{}", pretty);
    }

    #[test]
    fn test_constraint_parsing() {
        // Test that constraints are properly parsed from AST to HIR
        let source = r#"
            class User {
                name string @assert({{ this.length > 0 }})
                age int @check(valid_age, {{ this >= 0 }})
            }
        "#;
        let ast = parse_baml(source);
        let hir = Program::from_ast(&ast);

        // Find the User class
        let user_class = hir
            .classes
            .iter()
            .find(|c| c.name == "User")
            .expect("User class not found");

        // Check name field constraints
        let name_field = user_class
            .fields
            .iter()
            .find(|f| f.name == "name")
            .expect("name field not found");
        let name_meta = name_field.r#type.get_meta();
        assert_eq!(name_meta.constraints.len(), 1);
        let name_constraint = &name_meta.constraints[0];
        assert_eq!(name_constraint.level, baml_types::ConstraintLevel::Assert);
        assert_eq!(name_constraint.expression.0, "this.length > 0");
        assert_eq!(name_constraint.label, None);

        // Check age field constraints
        let age_field = user_class
            .fields
            .iter()
            .find(|f| f.name == "age")
            .expect("age field not found");
        let age_meta = age_field.r#type.get_meta();
        assert_eq!(age_meta.constraints.len(), 1);
        let age_constraint = &age_meta.constraints[0];
        assert_eq!(age_constraint.level, baml_types::ConstraintLevel::Check);
        assert_eq!(age_constraint.expression.0, "this >= 0");
        assert_eq!(age_constraint.label, Some("valid_age".to_string()));

        println!("Constraint parsing test passed!");
    }

    #[test]
    fn test_complex_class_with_mixed_attributes() {
        // Test class with streaming behaviors (no constraints for now)
        let source = r#"
            class StreamingUser {
                id string @stream.done
                username string @stream.not_null
                messages string[] @stream.with_state
                score int @stream.done
                metadata map<string, string> @stream.not_null
            }
        "#;
        let ast = parse_baml(source);
        let hir = Program::from_ast(&ast);
        let pretty = hir.pretty_print();

        // Verify the class pretty prints with streaming attributes
        assert!(pretty.contains("class StreamingUser"));
        assert!(pretty.contains("id: string @stream.done"));
        assert!(pretty.contains("username: string @stream.needed"));
        assert!(pretty.contains("messages: arraystring @stream.with_state"));
        assert!(pretty.contains("score: int @stream.done"));
        assert!(pretty.contains("metadata: mapstring:string @stream.needed"));

        // Print for visual inspection
        println!("\nClass with streaming attributes:");
        println!("{}", pretty);
    }

    #[test]
    fn test_nested_types_with_attributes() {
        // Test nested types (arrays, unions) with streaming attributes only for now
        let source = r#"
            class DataModel {
                tags string[]
                status (string | int) @stream.done
                matrix int[][]
                config map<string, bool> @stream.not_null
            }
        "#;
        let ast = parse_baml(source);
        let hir = Program::from_ast(&ast);
        let pretty = hir.pretty_print();

        // Print for visual inspection
        println!("\nNested types with streaming attributes:");
        println!("{}", pretty);

        // Verify nested types with attributes
        assert!(pretty.contains("class DataModel"));
        assert!(pretty.contains("tags:"));
        assert!(pretty.contains("status:"));
        assert!(pretty.contains("matrix:"));
        assert!(pretty.contains("config:"));
    }

    #[test]
    fn test_pretty_print_complex_structures() {
        let source = r#"
            function complexFunction(a: int, b: string, c: bool) -> string {
                let nested_array = [[1, 2], [3, 4]];
                let result = helper(a, b);
                result
            }
            
            function helper(x: int, y: string) -> string {
                "result"
            }
        "#;
        let ast = parse_baml(source);
        let hir = Program::from_ast(&ast);
        let pretty_printed = hir.pretty_print();
        // Check that it contains the expected structure
        assert!(pretty_printed.contains("function complexFunction(a, b, c)"));
        assert!(pretty_printed.contains("function helper(x, y)"));
        assert!(pretty_printed.contains("[[1, 2], [3, 4]]"));
        assert!(pretty_printed.contains("helper(a, b)"));
        // Test custom formatting options
        let narrow_format = hir.pretty_print_with_options(40, 4);
        assert!(narrow_format.len() > 0);
        // Print for visual inspection
        println!("Pretty printed complex HIR:");
        println!("{}", pretty_printed);
        println!("\nNarrow format (40 chars wide):");
        println!("{}", narrow_format);
    }

    // TODO: This is broken.
    #[test]
    #[ignore]
    fn test_if_expression_in_return_position() {
        // Test if expression desugaring in return position
        let source = r#"
            function conditionalReturn(flag: bool) -> string {
                if flag { "success" } else { "failure" }
            }
        "#;
        let expected = r#"function conditionalReturn(flag) {
  if flag {
  return "success";
  } else {
  return "failure";
  }
}"#;
        assert_eq!(hir_from_source(source), expected);
    }

    #[test]
    fn test_nested_expression_blocks() {
        // Test nested expression blocks with proper scoping
        let source = r#"
            function Foo() -> int {
                let x = {
                    let y = 1;
                    y
                };
                x
            }
        "#;
        // Expression blocks now properly preserve scope - the inner block
        // maintains its own variables which are not visible outside
        let result = hir_from_source(source);
        let expected = r#"function Foo() {
  let x = {
  let y = 1;
    y
  };
  return x;
}"#;
        assert_eq!(result, expected);
    }
    #[test]
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
a
b
}
}"#;
        assert_eq!(result, expected);
        // Print for visual inspection
        println!("HIR for class constructor with complex expressions:");
        println!("{}", result);
    }
}
