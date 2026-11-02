//! Statements inside a block.

use super::{AnyExpr, AnyPattern, Block, GenericParamBounds, Name, TestDef};
use crate::{
    AnyNode, Node, Walk,
    tree::{Bogus, Id, Missing},
};

#[derive(AnyNode, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnyStmt {
    Let(Id<LetStmt>),
    TypeBinding(Id<TypeBindingStmt>),
    Expr(Id<ExprStmt>),
    Return(Id<ReturnStmt>),
    Throw(Id<ThrowStmt>),
    Break(Id<BreakStmt>),
    Continue(Id<ContinueStmt>),
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

/// `let pattern = init else { … };` — also a top-level global, and the
/// initialiser of a C-style `for`.
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
pub struct ReturnStmt {
    pub kw: Token!["return"],
    pub value: Option<AnyExpr>,
    pub semi: Option<Token![";"]>,
}

/// `throw x;` — the expression is a `ThrowExpr`, or a `CatchExpr` wrapping
/// one (`throw x catch …`).
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ThrowStmt {
    pub expr: AnyExpr,
    pub semi: Option<Token![";"]>,
}

#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BreakStmt {
    pub kw: Token!["break"],
    pub semi: Option<Token![";"]>,
}

#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ContinueStmt {
    pub kw: Token!["continue"],
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

#[derive(AnyNode, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnyForHeader {
    In(Id<ForInHeader>),
    CStyle(Id<CForHeader>),
    Bogus(Id<Bogus>),
}

/// `(let x in xs)` or `let x in xs`.
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ForInHeader {
    pub l: Option<Token!["("]>,
    pub introducer: Token![LetIntroducer],
    pub pattern: Result<AnyPattern, Missing>,
    pub in_kw: Result<Token!["in"], Missing>,
    pub iter: Result<AnyExpr, Missing>,
    pub r: Option<Token![")"]>,
}

/// `(init; cond; step)` — every part optional. A `let` initialiser's `;` is
/// `semi1`, not the `LetStmt`'s.
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CForHeader {
    pub l: Token!["("],
    pub init: Option<AnyForInit>,
    pub semi1: Option<Token![";"]>,
    pub cond: Option<AnyExpr>,
    pub semi2: Option<Token![";"]>,
    pub step: Option<AnyExpr>,
    pub r: Result<Token![")"], Missing>,
}

#[derive(AnyNode, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnyForInit {
    Let(Id<LetStmt>),
    Expr(AnyExpr),
    Bogus(Id<Bogus>),
}

/// A `//#` header line inside a block (a section heading in the body).
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HeaderStmt {
    pub token: Token![Header],
}
