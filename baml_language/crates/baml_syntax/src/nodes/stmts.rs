//! Statements inside a block. A block's value is its tail expression; the
//! diverging forms (`return`, `throw`, `break`, `continue`) are expressions,
//! so in statement position they are an [`ExprStmt`] like any other.

use super::{AnyExpr, AnyPattern, Block, GenericParamBounds, Name, TestDef};
use crate::{
    AnyNode, Node, Walk,
    ops::AssignOpTok,
    tree::{Bogus, Id, Missing},
};

#[derive(AnyNode, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnyStmt {
    Let(Id<LetStmt>),
    TypeBinding(Id<TypeBindingStmt>),
    Expr(Id<ExprStmt>),
    Assign(Id<AssignStmt>),
    /// A stray `;`.
    Empty(Id<EmptyStmt>),
    Defer(Id<DeferStmt>),
    While(Id<WhileStmt>),
    WhileLet(Id<WhileLetStmt>),
    For(Id<ForStmt>),
    /// A `test`/`testset` nested in a testset.
    Test(Id<TestDef>),
    Header(Id<HeaderStmt>),
    Bogus(Id<Bogus>),
}

/// An expression in statement position, with its `;` if it has one. (An
/// `if`/`match` statement usually has none.)
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ExprStmt {
    pub expr: AnyExpr,
    pub semi: Option<Token![";"]>,
}

/// `target = value;`, or a compound `target += value;`. Assignment is a
/// statement: it has no value, so it is never an expression and never
/// chains (`a = b = c` is an error).
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AssignStmt {
    pub target: AnyExpr,
    pub op: AssignOpTok,
    pub value: Result<AnyExpr, Missing>,
    pub semi: Option<Token![";"]>,
}

/// A `;` with nothing before it.
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EmptyStmt {
    pub semi: Token![";"],
}

/// `let pattern = init else { … };` — also a top-level global, and the
/// initialiser of a C-style `for` (which owns the `;` then).
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LetStmt {
    pub introducer: Token![LetIntroducer],
    pub pattern: Result<AnyPattern, Missing>,
    pub eq: Result<Token!["="], Missing>,
    pub init: Result<AnyExpr, Missing>,
    pub else_branch: Option<LetElse>,
    pub semi: Option<Token![";"]>,
}

#[derive(Walk, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LetElse {
    pub kw: Token!["else"],
    pub block: Result<Id<Block>, Missing>,
}

/// `let type T extends A & B = expr else { … };`
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TypeBindingStmt {
    pub let_kw: Token!["let"],
    pub type_kw: Token!["type"],
    pub name: Result<Id<Name>, Missing>,
    pub bounds: Option<Id<GenericParamBounds>>,
    pub eq: Result<Token!["="], Missing>,
    pub init: Result<AnyExpr, Missing>,
    pub else_branch: Option<LetElse>,
    pub semi: Option<Token![";"]>,
}

#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DeferStmt {
    pub kw: Token!["defer"],
    pub body: Result<Id<Block>, Missing>,
}

#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WhileStmt {
    pub kw: Token!["while"],
    pub cond: Result<AnyExpr, Missing>,
    pub body: Result<Id<Block>, Missing>,
}

#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WhileLetStmt {
    pub kw: Token!["while"],
    pub introducer: Token![LetIntroducer],
    pub pattern: Result<AnyPattern, Missing>,
    pub eq: Result<Token!["="], Missing>,
    pub scrutinee: Result<AnyExpr, Missing>,
    pub body: Result<Id<Block>, Missing>,
}

#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ForStmt {
    pub kw: Token!["for"],
    pub header: Result<AnyForHeader, Missing>,
    pub body: Result<Id<Block>, Missing>,
}

/// The header of a `for`: the `in` form may be written bare or in
/// parentheses, the C-style form always in parentheses.
#[derive(AnyNode, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnyForHeader {
    In(Id<ForInHeader>),
    ParenIn(Id<ParenForInHeader>),
    CStyle(Id<CForHeader>),
    Bogus(Id<Bogus>),
}

/// `let x in xs`
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ForInHeader {
    pub introducer: Token![LetIntroducer],
    pub pattern: Result<AnyPattern, Missing>,
    pub in_kw: Result<Token!["in"], Missing>,
    pub iter: Result<AnyExpr, Missing>,
}

/// `(let x in xs)` — an opening parenthesis is always matched by a slot for
/// the closing one.
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ParenForInHeader {
    pub l: Token!["("],
    pub header: Id<ForInHeader>,
    pub r: Result<Token![")"], Missing>,
}

/// `(init; cond; step)` — every part optional, the parentheses required. A
/// `let` initialiser's `;` is `semi1`, not the `LetStmt`'s.
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CForHeader {
    pub l: Result<Token!["("], Missing>,
    pub init: Option<AnyForInit>,
    pub semi1: Option<Token![";"]>,
    pub cond: Option<AnyExpr>,
    pub semi2: Option<Token![";"]>,
    pub step: Option<AnyForStep>,
    pub r: Result<Token![")"], Missing>,
}

#[derive(AnyNode, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnyForInit {
    Let(Id<LetStmt>),
    Expr(AnyExpr),
    Bogus(Id<Bogus>),
}

/// The step of a C-style `for`: a statement position, so usually an
/// assignment (`i += 1`); its `AssignStmt` has no `;`.
#[derive(AnyNode, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnyForStep {
    Assign(Id<AssignStmt>),
    Expr(AnyExpr),
    Bogus(Id<Bogus>),
}

/// A `//#` header line inside a block (a section heading in the body): the
/// one trivia token a node owns, because here it is a statement. Anywhere
/// else — between the arms of a `match` included — it is an error.
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HeaderStmt {
    pub token: Token![HeaderComment],
}
