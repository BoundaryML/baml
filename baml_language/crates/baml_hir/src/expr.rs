//! Expression-level HIR.
//!
//! This module defines the arena-based expression IR for function bodies.
//! Expressions are stored in an arena and referenced by index (ExprId).

use std::collections::HashMap;
use std::sync::Arc;

use baml_base::{Name, Span};

use crate::TypeRef;

/// Index into the expression arena.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ExprId(pub u32);

impl ExprId {
    pub fn new(index: u32) -> Self {
        ExprId(index)
    }

    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// Index into the statement arena.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StmtId(pub u32);

impl StmtId {
    pub fn new(index: u32) -> Self {
        StmtId(index)
    }

    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// The body of an expression function.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExprBody {
    /// Arena of expressions.
    pub exprs: Vec<Expr>,
    /// Arena of statements.
    pub stmts: Vec<Stmt>,
    /// The root expression (return value of the function).
    pub root_expr: Option<ExprId>,
    /// Spans for each expression.
    pub expr_spans: HashMap<ExprId, Span>,
    /// Spans for each statement.
    pub stmt_spans: HashMap<StmtId, Span>,
}

impl ExprBody {
    /// Create an empty expression body.
    pub fn new() -> Self {
        ExprBody {
            exprs: Vec::new(),
            stmts: Vec::new(),
            root_expr: None,
            expr_spans: HashMap::new(),
            stmt_spans: HashMap::new(),
        }
    }

    /// Add an expression to the arena and return its ID.
    pub fn alloc_expr(&mut self, expr: Expr, span: Span) -> ExprId {
        let id = ExprId::new(self.exprs.len() as u32);
        self.exprs.push(expr);
        self.expr_spans.insert(id, span);
        id
    }

    /// Add a statement to the arena and return its ID.
    pub fn alloc_stmt(&mut self, stmt: Stmt, span: Span) -> StmtId {
        let id = StmtId::new(self.stmts.len() as u32);
        self.stmts.push(stmt);
        self.stmt_spans.insert(id, span);
        id
    }

    /// Get an expression by ID.
    pub fn get_expr(&self, id: ExprId) -> Option<&Expr> {
        self.exprs.get(id.index())
    }

    /// Get a statement by ID.
    pub fn get_stmt(&self, id: StmtId) -> Option<&Stmt> {
        self.stmts.get(id.index())
    }

    /// Get the span of an expression.
    pub fn expr_span(&self, id: ExprId) -> Option<Span> {
        self.expr_spans.get(&id).copied()
    }

    /// Get the span of a statement.
    pub fn stmt_span(&self, id: StmtId) -> Option<Span> {
        self.stmt_spans.get(&id).copied()
    }
}

impl Default for ExprBody {
    fn default() -> Self {
        Self::new()
    }
}

/// An expression in the HIR.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    /// A literal value.
    Literal(Literal),

    /// A variable reference.
    Path(Name),

    /// Binary operation: lhs op rhs
    Binary {
        lhs: ExprId,
        op: BinaryOp,
        rhs: ExprId,
    },

    /// Unary operation: op expr
    Unary { op: UnaryOp, expr: ExprId },

    /// Function call: callee(args...)
    Call { callee: ExprId, args: Vec<ExprId> },

    /// Method call: receiver.method(args...)
    MethodCall {
        receiver: ExprId,
        method: Name,
        args: Vec<ExprId>,
    },

    /// Field access: base.field
    Field { base: ExprId, field: Name },

    /// Index access: base[index]
    Index { base: ExprId, index: ExprId },

    /// Array literal: [elem1, elem2, ...]
    Array(Vec<ExprId>),

    /// Object literal: { field1: value1, ... } or TypeName { field1: value1, ... }
    Object {
        type_name: Option<Name>,
        fields: Vec<(Name, ExprId)>,
    },

    /// Block expression: { stmt1; stmt2; expr }
    Block {
        stmts: Vec<StmtId>,
        tail_expr: Option<ExprId>,
    },

    /// If expression: if cond { then } else { else }
    If {
        condition: ExprId,
        then_branch: ExprId,
        else_branch: Option<ExprId>,
    },

    /// Match expression: match expr { pat1 => expr1, ... }
    Match { expr: ExprId, arms: Vec<MatchArm> },

    /// Lambda/closure: |params| body
    Lambda {
        params: Vec<(Name, Option<TypeRef>)>,
        body: ExprId,
    },

    /// String interpolation: "hello {name}!"
    StringInterpolation(Vec<StringPart>),

    /// A missing or error expression (for recovery).
    Missing,
}

/// A literal value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Literal {
    Int(i64),
    Float(OrderedFloat),
    String(String),
    Bool(bool),
    Null,
}

/// Wrapper for f64 that implements Eq (for use in HashMaps etc.)
#[derive(Debug, Clone, Copy)]
pub struct OrderedFloat(pub f64);

impl PartialEq for OrderedFloat {
    fn eq(&self, other: &Self) -> bool {
        self.0.to_bits() == other.0.to_bits()
    }
}

impl Eq for OrderedFloat {}

impl std::hash::Hash for OrderedFloat {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.to_bits().hash(state);
    }
}

/// Binary operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BinaryOp {
    // Arithmetic
    Add,
    Sub,
    Mul,
    Div,
    Mod,

    // Comparison
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,

    // Logical
    And,
    Or,

    // String
    Concat,
}

/// Unary operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UnaryOp {
    Not,
    Neg,
}

/// A match arm.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub guard: Option<ExprId>,
    pub body: ExprId,
}

/// A pattern for matching.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pattern {
    /// Wildcard pattern: _
    Wildcard,
    /// Variable binding: name
    Binding(Name),
    /// Literal pattern: 42, "hello", true
    Literal(Literal),
    /// Variant pattern: EnumName.Variant
    Variant { enum_name: Name, variant: Name },
}

/// A part of a string interpolation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StringPart {
    /// Literal string part.
    Literal(String),
    /// Interpolated expression.
    Expr(ExprId),
}

/// A statement in the HIR.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stmt {
    /// Let binding: let name: type = expr
    Let {
        name: Name,
        type_annotation: Option<TypeRef>,
        initializer: Option<ExprId>,
    },

    /// Expression statement: expr;
    Expr(ExprId),

    /// Return statement: return expr
    Return(Option<ExprId>),
}

/// The body of a function - could be expression-based or LLM-based.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FunctionBody {
    /// An expression function with typed code.
    Expr(Arc<ExprBody>),
    /// An LLM function with a prompt template.
    Llm(LlmBody),
    /// Missing body (error recovery).
    Missing,
}

/// The body of an LLM function.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LlmBody {
    /// The client to use (e.g., "openai/gpt-4").
    pub client: Option<Name>,
    /// The prompt template.
    pub prompt: String,
}
