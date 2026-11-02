//! Backtick template strings. Block tags are structural: `${for …}` …
//! `${endfor}` is one [`TemplateFor`] with its body nested inside; a stray
//! closing tag is a `Bogus` part.

use super::{AnyExpr, AnyForHeader, AnyStmt};
use crate::{
    AnyNode, Node,
    tree::{Bogus, Id, List, Missing},
};

#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BacktickString {
    pub open: Token![BacktickStart],
    pub parts: List<AnyTemplatePart>,
    pub close: Result<Token![BacktickEnd], Missing>,
}

#[derive(AnyNode, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnyTemplatePart {
    /// Literal text, escapes included.
    Chunk(Token![TemplateChunk]),
    Interp(Id<TemplateInterp>),
    For(Id<TemplateFor>),
    If(Id<TemplateIf>),
    Bogus(Id<Bogus>),
}

/// `${ stmts; tail }` — an interpolation is a block body.
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TemplateInterp {
    pub open: Token!["${"],
    pub stmts: List<AnyStmt>,
    pub tail: Option<AnyExpr>,
    pub close: Result<Token!["}"], Missing>,
}

/// `${for …}` body `${endfor}`
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TemplateFor {
    pub open: Id<TemplateForOpen>,
    pub body: List<AnyTemplatePart>,
    pub close: Result<Id<TemplateEndTag>, Missing>,
}

#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TemplateForOpen {
    pub open: Token!["${"],
    pub kw: Token!["for"],
    pub header: Result<AnyForHeader, Missing>,
    pub close: Result<Token!["}"], Missing>,
}

/// `${if c}` … `${else if d}` … `${else}` … `${endif}`
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TemplateIf {
    pub open: Id<TemplateIfOpen>,
    pub then_parts: List<AnyTemplatePart>,
    pub elifs: List<Id<TemplateElif>>,
    pub else_part: Option<Id<TemplateElse>>,
    pub close: Result<Id<TemplateEndTag>, Missing>,
}

#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TemplateIfOpen {
    pub open: Token!["${"],
    pub kw: Token!["if"],
    pub cond: Result<AnyExpr, Missing>,
    pub close: Result<Token!["}"], Missing>,
}

/// `${else if d}` and the parts it guards.
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TemplateElif {
    pub open: Token!["${"],
    pub else_kw: Token!["else"],
    pub if_kw: Token!["if"],
    pub cond: Result<AnyExpr, Missing>,
    pub close: Result<Token!["}"], Missing>,
    pub parts: List<AnyTemplatePart>,
}

/// `${else}` and the parts it guards.
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TemplateElse {
    pub open: Token!["${"],
    pub kw: Token!["else"],
    pub close: Result<Token!["}"], Missing>,
    pub parts: List<AnyTemplatePart>,
}

/// `${endfor}` / `${endif}`
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TemplateEndTag {
    pub open: Token!["${"],
    pub kw: Token![EndTagKw],
    pub close: Result<Token!["}"], Missing>,
}
