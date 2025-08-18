//! Baml HIR.
//!
//! This file contains the definitions for all HIR items.

use baml_types::{type_meta::base::StreamingBehavior, Constraint};
use internal_baml_diagnostics::Span;

mod dump;
mod lowering;

/// High-level intermediate representation.
///
/// This is analogous to the HIR in Rust: https://rustc-dev-guide.rust-lang.org/hir.html
/// It carries just enough information to produce BAML bytecode. It differs from
/// baml-core IR in that it does not contain any type information. It has
/// limited metadata, for use in debugging, namely source spans.
///
/// See [`Hir::from_ast`] to see how BAML syntax is lowered into HIR.
///
/// Lowering from AST to HIR involves desugaring certain syntax forms.
///   - For loops become while loops.
///   - Class constructor spreads become regular class constructors with exhaustive fields.
///   - Implicit returns become explicit.
#[derive(Clone, Debug)]
pub struct Hir {
    pub expr_functions: Vec<ExprFunction>,
    pub llm_functions: Vec<LlmFunction>,
    pub classes: Vec<Class>,
    pub enums: Vec<Enum>,
    pub global_assignments: baml_types::BamlMap<String, Expression>,
}

pub type Type = TypeM<TypeMeta>;

#[derive(Clone, Debug)]
pub enum TypeM<M> {
    Int(M),
    String(M),
    Float(M),
    Bool(M),
    Null(M),
    Array(Box<TypeM<M>>, M),
    Map(Box<TypeM<M>>, Box<TypeM<M>>, M),
    ClassName(String, M),
    EnumName(String, M),
    Union(Vec<TypeM<M>>, M),
    Arrow(Arrow<M>, M),
}

impl<T: Default> TypeM<T> {
    pub fn int() -> Self {
        Self::Int(T::default())
    }
}

impl<T> TypeM<T> {
    pub fn name_for_user(&self) -> &'static str {
        match self {
            TypeM::Int(_) => "int",
            TypeM::String(_) => "string",
            TypeM::Float(_) => "float",
            TypeM::Bool(_) => "bool",
            TypeM::Null(_) => "null type",
            TypeM::Array(_, _) => "array",
            TypeM::Map(_, _, _) => "map",
            TypeM::ClassName(_, _) => "class",
            TypeM::EnumName(_, _) => "enum",
            TypeM::Union(_, _) => "union",
            TypeM::Arrow(_, _) => "function",
        }
    }
}

impl Type {
    /// Returns true if two types are exactly equal except for their spans.
    pub fn eq_up_to_span(&self, other: &Type) -> bool {
        match (self, other) {
            (TypeM::Int(a), TypeM::Int(b)) => a.eq_up_to_span(b),
            (TypeM::Float(a), TypeM::Float(b)) => a.eq_up_to_span(b),
            (TypeM::String(a), TypeM::String(b)) => a.eq_up_to_span(b),
            (TypeM::Bool(a), TypeM::Bool(b)) => a.eq_up_to_span(b),
            (TypeM::Null(a), TypeM::Null(b)) => a.eq_up_to_span(b),

            (TypeM::Array(a, a_meta), TypeM::Array(b, b_meta)) => {
                a.eq_up_to_span(b) && a_meta.eq_up_to_span(b_meta)
            }

            (TypeM::Map(a_key, a_val, a_meta), TypeM::Map(b_key, b_val, b_meta)) => {
                a_key.eq_up_to_span(b_key)
                    && a_val.eq_up_to_span(b_val)
                    && a_meta.eq_up_to_span(b_meta)
            }

            (TypeM::ClassName(a, a_meta), TypeM::ClassName(b, b_meta)) => {
                a == b && a_meta.eq_up_to_span(b_meta)
            }

            (TypeM::EnumName(a, a_meta), TypeM::EnumName(b, b_meta)) => {
                a == b && a_meta.eq_up_to_span(b_meta)
            }

            (TypeM::Union(a_members, a_meta), TypeM::Union(b_members, b_meta)) => {
                a_members.len() == b_members.len()
                    && a_members
                        .iter()
                        .zip(b_members.iter())
                        .all(|(a, b)| a.eq_up_to_span(b))
                    && a_meta.eq_up_to_span(b_meta)
            }

            (TypeM::Arrow(a_fn, a_meta), TypeM::Arrow(b_fn, b_meta)) => {
                a_fn.inputs.len() == b_fn.inputs.len()
                    && a_fn
                        .inputs
                        .iter()
                        .zip(b_fn.inputs.iter())
                        .all(|(a, b)| a.eq_up_to_span(b))
                    && a_fn.output.eq_up_to_span(&b_fn.output)
                    && a_meta.eq_up_to_span(b_meta)
            }

            _ => false,
        }
    }

    #[track_caller]
    pub fn can_be_assigned(&self, other: &Type) -> bool {
        // TODO: add diagnostics
        match (self, other) {
            (TypeM::Null(_), TypeM::Null(_))
            | (TypeM::Bool(_), TypeM::Bool(_))
            | (TypeM::Float(_), TypeM::Float(_))
            | (TypeM::String(_), TypeM::String(_))
            | (TypeM::Int(_), TypeM::Int(_)) => true,

            (TypeM::Array(a, _), TypeM::Array(b, _)) => a.can_be_assigned(b),

            (TypeM::Map(key_a, val_a, _), TypeM::Map(key_b, val_b, _)) => {
                key_a.can_be_assigned(key_b) && val_a.can_be_assigned(val_b)
            }

            (TypeM::EnumName(a, _), TypeM::EnumName(b, _))
            | (TypeM::ClassName(a, _), TypeM::ClassName(b, _)) => a == b,

            (TypeM::Union(a, _), TypeM::Union(b, _)) => {
                // there can't be any type in b that is not assignable to a.
                b.iter()
                    .all(|b_ty| a.iter().any(|a_ty| a_ty.can_be_assigned(b_ty)))
            }
            (TypeM::Union(inner, _), non_union) => {
                inner.iter().any(|i| i.can_be_assigned(non_union))
            }

            // for functions we only want the same inputs & same outputs, otherwise an
            // auto-cast mechanism would need to be in place.
            (a @ TypeM::Arrow(_, _), b @ TypeM::Arrow(_, _)) => a.eq_up_to_span(b),

            (_, _) => false,
        }
    }
    #[track_caller]
    pub fn assert_eq_up_to_span(&self, other: &Type) {
        match (self, other) {
            (TypeM::Int(a), TypeM::Int(b)) => assert!(a.eq_up_to_span(b)),
            (TypeM::Int(_), _) => panic!("Int type mismatch"),
            (TypeM::Float(a), TypeM::Float(b)) => assert!(a.eq_up_to_span(b)),
            (TypeM::Float(_), _) => panic!("Float type mismatch"),
            (TypeM::String(a), TypeM::String(b)) => assert!(a.eq_up_to_span(b)),
            (TypeM::String(_), _) => panic!("String type mismatch"),
            (TypeM::Bool(a), TypeM::Bool(b)) => assert!(a.eq_up_to_span(b)),
            (TypeM::Bool(_), _) => panic!("Bool type mismatch"),
            (TypeM::Null(a), TypeM::Null(b)) => assert!(a.eq_up_to_span(b)),
            (TypeM::Null(_), _) => panic!("Null type mismatch"),
            (TypeM::Array(a, a_meta), TypeM::Array(b, b_meta)) => {
                a.assert_eq_up_to_span(b);
                assert!(a_meta.eq_up_to_span(b_meta));
            }
            (TypeM::Array(_, _), _) => panic!("Array type mismatch"),
            (TypeM::Map(a, b, a_meta), TypeM::Map(c, d, b_meta)) => {
                a.assert_eq_up_to_span(c);
                b.assert_eq_up_to_span(d);
                assert!(a_meta.eq_up_to_span(b_meta));
            }
            (TypeM::Map(_, _, _), _) => panic!("Map type mismatch"),
            (TypeM::ClassName(a, a_meta), TypeM::ClassName(b, b_meta)) => {
                assert!(a == b);
                assert!(a_meta.eq_up_to_span(b_meta));
            }
            (TypeM::ClassName(_, _), _) => panic!("Class name type mismatch"),
            (TypeM::EnumName(a, a_meta), TypeM::EnumName(b, b_meta)) => {
                assert!(a == b);
                assert!(a_meta.eq_up_to_span(b_meta));
            }
            (TypeM::EnumName(_, _), _) => panic!("Enum name type mismatch"),
            (TypeM::Union(a, a_meta), TypeM::Union(b, b_meta)) => {
                assert!(a.len() == b.len());
                a.iter()
                    .zip(b.iter())
                    .for_each(|(a, b)| a.assert_eq_up_to_span(b));
                assert!(a_meta.eq_up_to_span(b_meta));
            }
            (TypeM::Union(_, _), _) => panic!("Union type mismatch"),
            (TypeM::Arrow(a, a_meta), TypeM::Arrow(b, b_meta)) => {
                assert!(a.inputs.len() == b.inputs.len());
                a.inputs
                    .iter()
                    .zip(b.inputs.iter())
                    .for_each(|(a, b)| a.assert_eq_up_to_span(b));
                a.output.assert_eq_up_to_span(&b.output);
                assert!(a_meta.eq_up_to_span(b_meta));
            }
            (TypeM::Arrow(_, _), _) => panic!("Arrow type mismatch"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Arrow<M> {
    pub inputs: Vec<TypeM<M>>,
    pub output: Box<TypeM<M>>,
}

#[derive(Clone, Debug)]
pub struct TypeMeta {
    pub span: Span,
    pub constraints: Vec<Constraint>,
    pub streaming_behavior: StreamingBehavior,
}

impl TypeMeta {
    #[track_caller]
    pub fn eq_up_to_span(&self, other: &TypeMeta) -> bool {
        self.constraints == other.constraints && self.streaming_behavior == other.streaming_behavior
    }

    #[track_caller]
    pub fn diagnose_eq_up_to_span(&self, other: &TypeMeta) -> anyhow::Result<()> {
        if self.constraints != other.constraints {
            return Err(anyhow::anyhow!("constraints do not match"));
        }
        if self.streaming_behavior != other.streaming_behavior {
            return Err(anyhow::anyhow!("streaming behaviors do not match"));
        }
        Ok(())
    }
}

impl Default for TypeMeta {
    fn default() -> Self {
        Self {
            span: Span::fake(),
            constraints: vec![],
            streaming_behavior: StreamingBehavior::default(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ExprFunction {
    pub name: String,
    pub parameters: Vec<Parameter>,
    pub return_type: TypeM<TypeMeta>,
    pub body: Block,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct LlmFunction {
    pub name: String,
    pub parameters: Vec<Parameter>,
    pub return_type: TypeM<TypeMeta>,
    pub client: String,
    pub prompt: String,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct Class {
    pub name: String,
    pub fields: Vec<Field>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct Field {
    pub name: String,
    pub r#type: TypeM<TypeMeta>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct Enum {
    pub name: String,
    pub variants: Vec<EnumVariant>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct EnumVariant {
    pub name: String,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct Parameter {
    pub name: String,
    pub is_mutable: bool,
    pub r#type: TypeM<TypeMeta>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct Block {
    pub statements: Vec<Statement>,
}

/// A single unit of execution within a block.
#[derive(Clone, Debug)]
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
    Declare {
        name: String,
        span: Span,
    },
    /// Assign a mutable variable.
    Assign {
        name: String,
        value: Expression,
        span: Span,
    },
    AssignOp {
        name: String,
        assign_op: AssignOp,
        value: Expression,
        span: Span,
    },
    /// Declare and assign a mutable reference in one statement.
    DeclareAndAssign {
        name: String,
        value: Expression,
        span: Span,
    },
    /// Return from a function.
    Return {
        expr: Expression,
        span: Span,
    },
    /// Evaluate an expression as the final value of a block (without returning from function).
    Expression {
        expr: Expression,
        span: Span,
    },
    SemicolonExpression {
        expr: Expression,
        span: Span,
    },
    While {
        condition: Expression,
        block: Block,
        span: Span,
    },
    ForLoop {
        identifier: String,
        iterator: Box<Expression>,
        block: Block,
        span: Span,
    },
    /// C-like for-loop that can't be directly mapped to `while` because it has either no condition or has after statement
    CForLoop {
        condition: Option<Expression>,
        after: Option<Box<Statement>>,
        block: Block,
    },
    Break(Span),
    Continue(Span),

    Assert {
        condition: Expression,
        span: Span,
    },
}

#[derive(Debug, Clone, Copy)]
pub enum AssignOp {
    /// The `+=` operator (addition)
    AddAssign,
    /// The `-=` operator (subtraction)
    SubAssign,
    /// The `*=` operator (multiplication)
    MulAssign,
    /// The `/=` operator (division)
    DivAssign,
    /// The `%=` operator (modulus)
    ModAssign,
    /// The `^=` operator (bitwise xor)
    BitXorAssign,
    /// The `&=` operator (bitwise and)
    BitAndAssign,
    /// The `|=` operator (bitwise or)
    BitOrAssign,
    /// The `<<=` operator (shift left)
    ShlAssign,
    /// The `>>=` operator (shift right)
    ShrAssign,
}

/// Expressions
#[derive(Clone, Debug)]
pub enum Expression {
    ArrayAccess {
        base: Box<Expression>,
        index: Box<Expression>,
        span: Span,
    },
    FieldAccess {
        base: Box<Expression>,
        field: String,
        span: Span,
    },
    MethodCall {
        receiver: Box<Expression>,
        method: String,
        args: Vec<Expression>,
        span: Span,
    },
    BoolValue(bool, Span),
    NumericValue(String, Span),
    Identifier(String, Span),
    StringValue(String, Span),
    RawStringValue(String, Span),
    If {
        condition: Box<Expression>,
        if_branch: Box<Expression>,
        else_branch: Option<Box<Expression>>,
        span: Span,
    },
    Array(Vec<Expression>, Span),
    Map(Vec<(Expression, Expression)>, Span),
    JinjaExpressionValue(String, Span),
    Call {
        function: Box<Expression>,
        type_args: Vec<TypeArg>,
        args: Vec<Expression>,
        span: Span,
    },
    // Lambda(ArgumentsList, Box<ExpressionBlock>, Span), // TODO.
    // MethodCall(Box<Expression>, String, Vec<Expression>), // TODO.
    ClassConstructor(ClassConstructor, Span),
    /// Expression block - has its own scope with statements and evaluates to a value
    ExpressionBlock(Block, Span),
    BinaryOperation {
        left: Box<Expression>,
        operator: BinaryOperator,
        right: Box<Expression>,
        span: Span,
    },
    UnaryOperation {
        operator: UnaryOperator,
        expr: Box<Expression>,
        span: Span,
    },
    Paren(Box<Expression>, Span),
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum BinaryOperator {
    /// The `==` operator (equal).
    Eq,
    /// The `!=` operator (not equal).
    Neq,
    /// The `<` operator (less than).
    Lt,
    /// The `<=` operator (less than or equal).
    LtEq,
    /// The `>` operator (greater than).
    Gt,
    /// The `>=` operator (greater than or equal).
    GtEq,
    /// The `+` operator (addition).
    Add,
    /// The `-` operator (subtraction).
    Sub,
    /// The `*` operator (multiplication).
    Mul,
    /// The `/` operator (division).
    Div,
    /// The `%` operator (modulus).
    Mod,
    /// The `&` operator (bitwise and).
    BitAnd,
    /// The `|` operator (bitwise or).
    BitOr,
    /// The `^` operator (bitwise xor).
    BitXor,
    /// The `<<` operator (shift left).
    Shl,
    /// The `>>` operator (shift right).
    Shr,
    /// The `&&` operator (logical and).
    And,
    /// The `||` operator (logical or).
    Or,
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum UnaryOperator {
    Not,
    Neg,
}

/// A type argument to a generic function call.
///
/// std.fetch_value<int>(...) == TypeArg::Type(int),
/// std.fetch_value<T>(...) == TypeArg::TypeName("T")
#[derive(Clone, Debug)]
pub enum TypeArg {
    Type(TypeM<TypeMeta>),
    TypeName(String),
}

// TODO: struct Expr {kind: ExprKind, span: Span}
impl Expression {
    pub fn span(&self) -> Span {
        match self {
            Expression::ArrayAccess { span, .. } => span.clone(),
            Expression::FieldAccess { span, .. } => span.clone(),
            Expression::MethodCall { span, .. } => span.clone(),
            Expression::BoolValue(_, span) => span.clone(),
            Expression::NumericValue(_, span) => span.clone(),
            Expression::Identifier(_, span) => span.clone(),
            Expression::StringValue(_, span) => span.clone(),
            Expression::RawStringValue(_, span) => span.clone(),
            Expression::If { span, .. } => span.clone(),
            Expression::Array(_, span) => span.clone(),
            Expression::Map(_, span) => span.clone(),
            Expression::JinjaExpressionValue(_, span) => span.clone(),
            Expression::Call { span, .. } => span.clone(),
            Expression::ClassConstructor(_, span) => span.clone(),
            Expression::ExpressionBlock(_, span) => span.clone(),
            Expression::BinaryOperation { span, .. } => span.clone(),
            Expression::UnaryOperation { span, .. } => span.clone(),
            Expression::Paren(_, span) => span.clone(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ClassConstructor {
    pub class_name: String,
    pub fields: Vec<ClassConstructorField>,
}

#[derive(Clone, Debug)]
pub enum ClassConstructorField {
    Named { name: String, value: Expression },
    Spread { value: Expression },
}
