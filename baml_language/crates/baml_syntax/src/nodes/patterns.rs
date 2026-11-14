//! Patterns. The `let`/`const` introducer of a binding statement lives on the
//! statement (`LetStmt`, `IfLetExpr`, …); a pattern carries one only where it
//! introduces the binding itself (`match x { let y => … }`, `[let a, ..]`).

use super::{AnyType, Name, PathType, TypeAscription};
use crate::{
    AnyNode, Node, Walk,
    token::punct,
    tree::{Bogus, Id, List1, Missing, Sep},
};

#[derive(AnyNode, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnyPattern {
    Binding(Id<BindingPattern>),
    Wildcard(Id<WildcardPattern>),
    Destructure(Id<DestructurePattern>),
    Array(Id<ArrayPattern>),
    Type(Id<TypePattern>),
    Paren(Id<ParenPattern>),
    Union(Id<UnionPattern>),
    Bogus(Id<Bogus>),
}

/// `let x`, `x`, `let x: int | string`.
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BindingPattern {
    pub introducer: Option<Token![LetIntroducer]>,
    pub name: Id<Name>,
    pub ascription: Option<PatternAscription>,
}

/// `: pattern` — narrows a binding, or gives a field its sub-pattern.
#[derive(Walk, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PatternAscription {
    pub colon: Token![":"],
    pub pattern: Result<AnyPattern, Missing>,
}

/// `_` or `let _`.
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WildcardPattern {
    pub introducer: Option<Token![LetIntroducer]>,
    pub underscore: Token!["_"],
}

/// `let Point<T> { x, y: let py }`
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DestructurePattern {
    pub introducer: Option<Token![LetIntroducer]>,
    pub path: Id<PathType>,
    pub l: Token!["{"],
    pub fields: Sep<Id<FieldPattern>, punct::Comma>,
    pub r: Result<Token!["}"], Missing>,
}

/// `x` (shorthand) or `x: pattern`.
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FieldPattern {
    pub name: Id<Name>,
    pub sub: Option<PatternAscription>,
}

/// `[a, ..let rest]: T[]`
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ArrayPattern {
    pub l: Token!["["],
    pub elements: Sep<AnyArrayPatternElement, punct::Comma>,
    pub r: Result<Token!["]"], Missing>,
    pub ascription: Option<Id<TypeAscription>>,
}

#[derive(AnyNode, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnyArrayPatternElement {
    Pattern(AnyPattern),
    Rest(Id<RestPattern>),
    Bogus(Id<Bogus>),
}

/// `..` or `..pattern`
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RestPattern {
    pub dots: Token![".."],
    pub pattern: Option<AnyPattern>,
}

/// A type used as a pattern: `int`, `"a"`, `-1`, `Foo<T>[]`.
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TypePattern {
    pub ty: AnyType,
}

#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ParenPattern {
    pub l: Token!["("],
    pub inner: Result<AnyPattern, Missing>,
    pub r: Result<Token![")"], Missing>,
}

/// `A | B | C`
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UnionPattern {
    pub first: AnyPattern,
    /// `| P` at least once; a trailing `|` is an arm whose pattern is missing.
    pub arms: List1<PatternArm>,
}

#[derive(Walk, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PatternArm {
    pub pipe: Token!["|"],
    pub pattern: Result<AnyPattern, Missing>,
}
