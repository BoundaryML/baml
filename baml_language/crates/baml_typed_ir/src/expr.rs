//! Expression-only typed IR.
//!
//! All constructs are expressions that return values. This eliminates the
//! statement/expression distinction and makes traversals uniform.
//!
//! # No Missing Nodes
//!
//! Unlike HIR which has `Missing` variants for LSP error recovery, `TypedIR`
//! represents only **valid, complete programs**. If the HIR contains any
//! `Missing` nodes, lowering will fail. This is the gate between
//! "LSP-compatible IR" and "codegen-ready IR".

use baml_base::Name;
use la_arena::{Arena, Idx};
use text_size::TextRange;

use crate::Ty;

/// Expression ID - index into the expression arena.
pub type ExprId = Idx<Expr>;

/// Pattern ID - index into the pattern arena.
pub type PatId = Idx<Pattern>;

/// A typed expression body containing all expressions for a function.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExprBody {
    /// Expression arena - all expressions allocated here.
    pub exprs: Arena<Expr>,
    /// Pattern arena - for let bindings.
    pub patterns: Arena<Pattern>,
    /// Type for each expression.
    pub expr_types: rustc_hash::FxHashMap<ExprId, Ty>,
    /// Source spans for expressions.
    pub expr_spans: rustc_hash::FxHashMap<ExprId, TextRange>,
    /// Root expression of the body.
    pub root: ExprId,
}

impl ExprBody {
    /// Get the type of an expression.
    pub fn ty(&self, id: ExprId) -> &Ty {
        self.expr_types
            .get(&id)
            .expect("all expressions have types")
    }

    /// Get the span of an expression.
    pub fn span(&self, id: ExprId) -> Option<TextRange> {
        self.expr_spans.get(&id).copied()
    }

    /// Get an expression by ID.
    pub fn expr(&self, id: ExprId) -> &Expr {
        &self.exprs[id]
    }

    /// Get a pattern by ID.
    pub fn pattern(&self, id: PatId) -> &Pattern {
        &self.patterns[id]
    }
}

/// Unified expression type - everything is an expression.
///
/// # Design
///
/// Unlike traditional ASTs that separate statements from expressions,
/// this IR treats everything uniformly:
///
/// - `Let` binds a variable in a scope and returns the scope's value
/// - `Seq` sequences two expressions, returning the second's value
/// - `While` loops and returns Unit
/// - `Assign` assigns and returns Unit
/// - `If` branches and returns the chosen branch's value
///
/// This makes traversals much simpler - there's only one visitor method.
///
/// # No Error Recovery
///
/// There is no `Missing` variant. `TypedIR` only represents valid programs.
/// If the source has errors, lowering from HIR will fail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    // ========== Literals ==========
    /// Literal value.
    Literal(Literal),

    /// Unit value - the result of effectful expressions like `while`, `assign`.
    Unit,

    // ========== Variables & Paths ==========
    /// Variable reference.
    Var(Name),

    /// Multi-segment path (e.g., `user.name`, `Status.Active`).
    Path(Vec<Name>),

    // ========== Binding & Sequencing ==========
    /// Let binding: `let pattern: ty = value in body`
    ///
    /// Binds the pattern with type `ty` to `value`, then evaluates `body`.
    /// Returns the value of `body`.
    Let {
        pattern: PatId,
        ty: Ty,
        value: ExprId,
        body: ExprId,
    },

    /// Sequence: evaluate `first` for side effects, return `second`.
    ///
    /// A block like `{ stmt1; stmt2; expr }` becomes:
    /// `Seq(stmt1, Seq(stmt2, expr))`
    Seq { first: ExprId, second: ExprId },

    // ========== Control Flow ==========
    /// If expression: `if cond { then } else { else_ }`
    ///
    /// Returns the value of the chosen branch.
    /// If `else_` is None, returns Unit when condition is false.
    If {
        condition: ExprId,
        then_branch: ExprId,
        else_branch: Option<ExprId>,
    },

    /// While loop: `while cond { body }`
    ///
    /// Returns Unit after loop terminates.
    While { condition: ExprId, body: ExprId },

    /// Return from function. Diverges (type is Never).
    Return(Option<ExprId>),

    /// Break from loop. Diverges (type is Never).
    Break,

    /// Continue to next iteration. Diverges (type is Never).
    Continue,

    // ========== Assignment ==========
    /// Simple assignment: `target = value`. Returns Unit.
    Assign { target: ExprId, value: ExprId },

    /// Compound assignment: `target op= value`. Returns Unit.
    AssignOp {
        target: ExprId,
        op: AssignOp,
        value: ExprId,
    },

    // ========== Operations ==========
    /// Binary operation.
    Binary {
        op: BinaryOp,
        lhs: ExprId,
        rhs: ExprId,
    },

    /// Unary operation.
    Unary { op: UnaryOp, operand: ExprId },

    // ========== Function Calls ==========
    /// Function call: `callee(args...)`
    Call { callee: ExprId, args: Vec<ExprId> },

    // ========== Data Structures ==========
    /// Array literal: `[elem1, elem2, ...]`
    Array { elements: Vec<ExprId> },

    /// Object/struct literal: `TypeName { field1: value1, ... }`
    Object {
        type_name: Option<Name>,
        fields: Vec<(Name, ExprId)>,
    },

    // ========== Access ==========
    /// Field access: `base.field`
    FieldAccess { base: ExprId, field: Name },

    /// Index access: `base[index]`
    Index { base: ExprId, index: ExprId },

    /// Match expression: `match scrutinee { arm1, arm2, ... }`
    ///
    /// Pattern matching with exhaustiveness checking.
    Match {
        scrutinee: ExprId,
        arms: Vec<MatchArm>,
    },
}

/// A single arm in a match expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchArm {
    /// The pattern to match against.
    pub pattern: PatId,
    /// Optional guard: `if condition`
    pub guard: Option<ExprId>,
    /// The body expression (result if this arm matches).
    pub body: ExprId,
}

/// Literal values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Literal {
    Int(i64),
    Float(String), // Store as string to preserve precision
    String(String),
    Bool(bool),
    Null,
}

/// Binary operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

    // Bitwise
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
}

/// Unary operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Not,
    Neg,
}

/// Compound assignment operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignOp {
    Add,    // +=
    Sub,    // -=
    Mul,    // *=
    Div,    // /=
    Mod,    // %=
    BitAnd, // &=
    BitOr,  // |=
    BitXor, // ^=
    Shl,    // <<=
    Shr,    // >>=
}

/// Binding pattern.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pattern {
    /// Simple variable binding: `x`, `user`, `_`
    Binding(Name),

    /// Typed binding pattern: `s: Success`, `n: int`
    TypedBinding { name: Name, ty: crate::Ty },

    /// Literal pattern: `null`, `true`, `false`, `42`, `3.14`, `"hello"`
    Literal(Literal),

    /// Enum variant pattern: `Status.Active`
    EnumVariant { enum_name: Name, variant: Name },

    /// Union pattern: `200 | 201 | 204` or `Status.Active | Status.Pending`
    Union(Vec<PatId>),
}

// ============================================================================
// Conversion helpers from HIR types
// ============================================================================

impl From<baml_hir::BinaryOp> for BinaryOp {
    fn from(op: baml_hir::BinaryOp) -> Self {
        match op {
            baml_hir::BinaryOp::Add => BinaryOp::Add,
            baml_hir::BinaryOp::Sub => BinaryOp::Sub,
            baml_hir::BinaryOp::Mul => BinaryOp::Mul,
            baml_hir::BinaryOp::Div => BinaryOp::Div,
            baml_hir::BinaryOp::Mod => BinaryOp::Mod,
            baml_hir::BinaryOp::Eq => BinaryOp::Eq,
            baml_hir::BinaryOp::Ne => BinaryOp::Ne,
            baml_hir::BinaryOp::Lt => BinaryOp::Lt,
            baml_hir::BinaryOp::Le => BinaryOp::Le,
            baml_hir::BinaryOp::Gt => BinaryOp::Gt,
            baml_hir::BinaryOp::Ge => BinaryOp::Ge,
            baml_hir::BinaryOp::And => BinaryOp::And,
            baml_hir::BinaryOp::Or => BinaryOp::Or,
            baml_hir::BinaryOp::BitAnd => BinaryOp::BitAnd,
            baml_hir::BinaryOp::BitOr => BinaryOp::BitOr,
            baml_hir::BinaryOp::BitXor => BinaryOp::BitXor,
            baml_hir::BinaryOp::Shl => BinaryOp::Shl,
            baml_hir::BinaryOp::Shr => BinaryOp::Shr,
        }
    }
}

impl From<baml_hir::UnaryOp> for UnaryOp {
    fn from(op: baml_hir::UnaryOp) -> Self {
        match op {
            baml_hir::UnaryOp::Not => UnaryOp::Not,
            baml_hir::UnaryOp::Neg => UnaryOp::Neg,
        }
    }
}

impl From<baml_hir::AssignOp> for AssignOp {
    fn from(op: baml_hir::AssignOp) -> Self {
        match op {
            baml_hir::AssignOp::Add => AssignOp::Add,
            baml_hir::AssignOp::Sub => AssignOp::Sub,
            baml_hir::AssignOp::Mul => AssignOp::Mul,
            baml_hir::AssignOp::Div => AssignOp::Div,
            baml_hir::AssignOp::Mod => AssignOp::Mod,
            baml_hir::AssignOp::BitAnd => AssignOp::BitAnd,
            baml_hir::AssignOp::BitOr => AssignOp::BitOr,
            baml_hir::AssignOp::BitXor => AssignOp::BitXor,
            baml_hir::AssignOp::Shl => AssignOp::Shl,
            baml_hir::AssignOp::Shr => AssignOp::Shr,
        }
    }
}

impl From<&baml_hir::Literal> for Literal {
    fn from(lit: &baml_hir::Literal) -> Self {
        match lit {
            baml_hir::Literal::Int(n) => Literal::Int(*n),
            baml_hir::Literal::Float(s) => Literal::Float(s.clone()),
            baml_hir::Literal::String(s) => Literal::String(s.clone()),
            baml_hir::Literal::Bool(b) => Literal::Bool(*b),
            baml_hir::Literal::Null => Literal::Null,
        }
    }
}
