//! Type expressions. Modifiers and unions are nodes, not token soup: `int[]?`
//! is `Optional(Array(Path(int)))`.

use super::{BacktickString, Name, PathSegment, ThrowsClause, TypeArgs};
use crate::{
    AnyNode, Node, Walk,
    token::punct,
    tree::{Bogus, Id, List, List1, Missing, Sep},
};

#[derive(AnyNode, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnyType {
    Path(Id<PathType>),
    Literal(Id<LiteralType>),
    Paren(Id<ParenType>),
    Tuple(Id<TupleType>),
    Function(Id<FunctionType>),
    Optional(Id<OptionalType>),
    Array(Id<ArrayType>),
    Union(Id<UnionType>),
    Qualified(Id<QualifiedPath>),
    Bogus(Id<Bogus>),
}

/// `int`, `map<K, V>`, `baml.Image`, `Self.Item`, `Future<T, E>`.
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PathType {
    pub head: Id<Name>,
    pub segments: List<PathSegment>,
    pub args: Option<Id<TypeArgs>>,
}

/// `"a"`, `-1`, `10n`, or a backtick string literal type.
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LiteralType {
    pub minus: Option<Token!["-"]>,
    pub value: AnyLiteralTypeValue,
}

#[derive(AnyNode, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnyLiteralTypeValue {
    Scalar(Token![LiteralTypeToken]),
    Backtick(Id<BacktickString>),
    Bogus(Id<Bogus>),
}

/// `(A | B)` — grouping only; a parenthesised list with an arrow is a
/// [`FunctionType`].
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ParenType {
    pub l: Token!["("],
    pub inner: Result<AnyType, Missing>,
    pub r: Result<Token![")"], Missing>,
}

/// `(a: A, B) -> R throws E`
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FunctionType {
    pub l: Token!["("],
    pub params: Sep<Id<FunctionTypeParam>, punct::Comma>,
    pub r: Result<Token![")"], Missing>,
    pub arrow: Token!["->"],
    pub ret: Result<AnyType, Missing>,
    pub throws: Option<Id<ThrowsClause>>,
}

/// `T`, or `name: T` / `name?: T` for a named (optionally omittable) parameter.
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FunctionTypeParam {
    pub name: Option<FunctionTypeParamName>,
    pub ty: Result<AnyType, Missing>,
}

#[derive(Walk, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FunctionTypeParamName {
    pub name: Id<Name>,
    pub question: Option<Token!["?"]>,
    pub colon: Token![":"],
}

/// `T?`
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OptionalType {
    pub inner: AnyType,
    pub q: Token!["?"],
}

/// `T[]`
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ArrayType {
    pub inner: AnyType,
    pub l: Token!["["],
    pub r: Result<Token!["]"], Missing>,
}

/// `A | B | C` — members are never unions themselves.
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UnionType {
    pub first: AnyType,
    /// `| T` at least once; a trailing `|` is an arm whose type is missing.
    pub arms: List1<UnionArm>,
}

#[derive(Walk, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UnionArm {
    pub pipe: Token!["|"],
    pub ty: Result<AnyType, Missing>,
}

/// `(Base as Interface).item` — the same spelling names an associated type in
/// type position and an associated item in expression position, so one node
/// serves both.
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct QualifiedPath {
    pub l: Token!["("],
    pub base: AnyType,
    pub as_kw: Token!["as"],
    pub iface: Result<AnyType, Missing>,
    pub r: Result<Token![")"], Missing>,
    pub dot: Result<Token!["."], Missing>,
    pub member: Result<Id<Name>, Missing>,
}

#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TupleType {
    pub l: Token!["("],
    pub elements: Sep<AnyType, punct::Comma>,
    pub r: Result<Token![")"], Missing>,
}
