//! Expressions. Every operand is a node: a lone identifier is a single-segment
//! [`PathExpr`], a number or `true` is a [`LiteralExpr`].

use super::{
    AnyPattern, AnyStmt, AnyType, BacktickString, DottedName, GenericArgs, ParamList,
    QualifiedPath, ThrowsClause, TypeAscription, WithClause,
};
use crate::{
    AnyNode, Node, Walk,
    ops::BinaryOpTok,
    token::punct,
    tree::{Bogus, Id, List, List1, Missing, Sep},
};

#[derive(AnyNode, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnyExpr {
    Path(Id<PathExpr>),
    GenericApply(Id<GenericApplyExpr>),
    Literal(Id<LiteralExpr>),
    String(Id<StringLit>),
    Backtick(Id<BacktickString>),
    FieldAccess(Id<FieldAccessExpr>),
    OptionalFieldAccess(Id<OptionalFieldAccessExpr>),
    Upcast(Id<UpcastExpr>),
    QualifiedPath(Id<QualifiedPath>),
    Spec(Id<SpecExpr>),
    EnvAccess(Id<EnvAccessExpr>),
    Call(Id<CallExpr>),
    OptionalCall(Id<OptionalCallExpr>),
    Index(Id<IndexExpr>),
    OptionalIndex(Id<OptionalIndexExpr>),
    TaggedTemplate(Id<TaggedTemplateExpr>),
    Binary(Id<BinaryExpr>),
    Is(Id<IsExpr>),
    Unary(Id<UnaryExpr>),
    Await(Id<AwaitExpr>),
    Spawn(Id<SpawnExpr>),
    Catch(Id<CatchExpr>),
    Throw(Id<ThrowExpr>),
    Return(Id<ReturnExpr>),
    Break(Id<BreakExpr>),
    Continue(Id<ContinueExpr>),
    Paren(Id<ParenExpr>),
    Block(Id<Block>),
    If(Id<IfExpr>),
    IfLet(Id<IfLetExpr>),
    Match(Id<MatchExpr>),
    Lambda(Id<LambdaExpr>),
    Array(Id<ArrayLiteral>),
    Map(Id<MapLiteral>),
    Object(Id<ObjectLiteral>),
    Bogus(Id<Bogus>),
}

/// A name: an identifier, a contextual keyword, or a reserved word in a
/// position that accepts it.
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Name {
    pub token: Token![NameToken],
}

/// `x`, `user.name`, `baml.HttpMethod.Get` — what the segments refer to is
/// decided by name resolution, not the parser.
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PathExpr {
    pub head: Id<Name>,
    pub segments: List<PathSegment>,
}

/// `.name` after a path head, in expressions, types and attribute names
/// alike. A dot with nothing after it is a segment with a missing name — a
/// trailing dot is never valid, so the shape leaves no silent place for one.
#[derive(Walk, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PathSegment {
    pub dot: Token!["."],
    pub name: Result<Id<Name>, Missing>,
}

/// `foo<T>`, `a.b<T>`, `f().g<T>`.
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GenericApplyExpr {
    pub base: AnyExpr,
    pub args: Id<GenericArgs>,
}

/// `1`, `1.5`, `10n`, `true`, `false`, `null`.
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LiteralExpr {
    pub token: Token![LiteralExprToken],
}

/// `"…"`, `r"…"`, `b"…"`, or the removed `#"…"#`; one token each.
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StringLit {
    pub token: Token![StringToken],
}

/// `expr.name` on a computed base (`f().x`, `a[0].x`).
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FieldAccessExpr {
    pub base: AnyExpr,
    pub dot: Token![MemberDot],
    pub name: Result<Id<Name>, Missing>,
}

/// `expr?.name`
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OptionalFieldAccessExpr {
    pub base: AnyExpr,
    pub qdot: Token!["?."],
    pub name: Result<Id<Name>, Missing>,
}

/// `expr.as<T>`
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UpcastExpr {
    pub base: AnyExpr,
    pub dot: Token!["."],
    pub as_kw: Token!["as"],
    pub args: Result<Id<GenericArgs>, Missing>,
}

/// `F@spec`, `F@stream`, …
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SpecExpr {
    pub base: AnyExpr,
    pub at: Token!["@"],
    pub selector: Result<Token![SpecSelector], Missing>,
}

/// `env.NAME`
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EnvAccessExpr {
    pub env: Token!["env"],
    pub dot: Token!["."],
    pub name: Result<Id<Name>, Missing>,
}

#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CallExpr {
    pub callee: AnyExpr,
    pub args: Id<ArgList>,
}

#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ArgList {
    pub l: Token!["("],
    pub args: Sep<Id<Arg>, punct::Comma>,
    pub r: Result<Token![")"], Missing>,
}

/// `value` or `label = value`.
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Arg {
    pub label: Option<ArgLabel>,
    pub value: Result<AnyExpr, Missing>,
}

#[derive(Walk, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ArgLabel {
    pub name: Id<Name>,
    pub eq: Token!["="],
}

/// `f?.(args)`
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OptionalCallExpr {
    pub callee: AnyExpr,
    pub qdot: Token!["?."],
    pub args: Id<ArgList>,
}

#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct IndexExpr {
    pub base: AnyExpr,
    pub l: Token!["["],
    pub index: Result<AnyExpr, Missing>,
    pub r: Result<Token!["]"], Missing>,
}

/// `a?.[i]`
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OptionalIndexExpr {
    pub base: AnyExpr,
    pub qdot: Token!["?."],
    pub l: Token!["["],
    pub index: Result<AnyExpr, Missing>,
    pub r: Result<Token!["]"], Missing>,
}

/// `` tag`…` ``
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TaggedTemplateExpr {
    pub tag: AnyExpr,
    pub template: Id<BacktickString>,
}

/// `lhs op rhs`, assignment included.
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BinaryExpr {
    pub lhs: AnyExpr,
    pub op: BinaryOpTok,
    pub rhs: Result<AnyExpr, Missing>,
}

/// `expr is pattern`
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct IsExpr {
    pub expr: AnyExpr,
    pub is_kw: Token!["is"],
    pub pattern: Result<AnyPattern, Missing>,
}

/// `-x`, `!x`, `~x`, `++x`, `--x`; the operator is
/// [`UnaryOp::from_kind`](crate::ops::UnaryOp::from_kind) of the token.
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UnaryExpr {
    pub op: Token![UnaryOpToken],
    pub operand: Result<AnyExpr, Missing>,
}

#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AwaitExpr {
    pub kw: Token!["await"],
    pub operand: Result<AnyExpr, Missing>,
}

/// `spawn name with a, b { … }`
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SpawnExpr {
    pub kw: Token!["spawn"],
    pub name: Option<AnyExpr>,
    pub with: Option<Id<WithClause>>,
    pub body: Result<Id<Block>, Missing>,
}

/// `expr catch (e) { … } catch_all (e) { … }`
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CatchExpr {
    pub body: AnyExpr,
    pub clauses: List1<Id<CatchClause>>,
}

#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CatchClause {
    pub kw: Token![CatchKw],
    pub l: Result<Token!["("], Missing>,
    pub binding: Result<Id<Name>, Missing>,
    pub trace: Option<TraceBinding>,
    pub r: Result<Token![")"], Missing>,
    pub lbrace: Result<Token!["{"], Missing>,
    pub arms: List<Id<CatchArm>>,
    pub rbrace: Result<Token!["}"], Missing>,
}

/// `, trace` — the stack-trace binding of a catch clause.
#[derive(Walk, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TraceBinding {
    pub comma: Token![","],
    pub name: Result<Id<Name>, Missing>,
}

#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CatchArm {
    pub pattern: Result<AnyPattern, Missing>,
    pub arrow: Result<Token!["=>"], Missing>,
    pub body: Result<AnyExpr, Missing>,
    pub comma: Option<Token![","]>,
}

/// `throw payload` — a diverging expression; in statement position it is an
/// `ExprStmt`. The payload is required.
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ThrowExpr {
    pub kw: Token!["throw"],
    pub payload: Result<AnyExpr, Missing>,
}

/// `return` or `return value` — diverging; a statement is an `ExprStmt`.
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ReturnExpr {
    pub kw: Token!["return"],
    pub value: Option<AnyExpr>,
}

#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BreakExpr {
    pub kw: Token!["break"],
}

#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ContinueExpr {
    pub kw: Token!["continue"],
}

#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ParenExpr {
    pub l: Token!["("],
    pub inner: Result<AnyExpr, Missing>,
    pub r: Result<Token![")"], Missing>,
}

/// `{ stmt; stmt; tail }` — the tail is the final expression with no `;`,
/// the block's value.
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Block {
    pub l: Token!["{"],
    pub stmts: List<AnyStmt>,
    pub tail: Option<AnyExpr>,
    pub r: Result<Token!["}"], Missing>,
}

/// `if cond { … } else …` — `if (c)` keeps its parentheses as a
/// [`ParenExpr`] condition.
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct IfExpr {
    pub kw: Token!["if"],
    pub cond: Result<AnyExpr, Missing>,
    pub then_branch: Result<Id<Block>, Missing>,
    pub else_branch: Option<ElseBranch>,
}

#[derive(Walk, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ElseBranch {
    pub kw: Token!["else"],
    pub body: Result<AnyElse, Missing>,
}

#[derive(AnyNode, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnyElse {
    Block(Id<Block>),
    If(Id<IfExpr>),
    IfLet(Id<IfLetExpr>),
    Bogus(Id<Bogus>),
}

/// `if let pattern = scrutinee { … } else …`
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct IfLetExpr {
    pub kw: Token!["if"],
    pub introducer: Token![LetIntroducer],
    pub pattern: Result<AnyPattern, Missing>,
    pub eq: Result<Token!["="], Missing>,
    pub scrutinee: Result<AnyExpr, Missing>,
    pub then_branch: Result<Id<Block>, Missing>,
    pub else_branch: Option<ElseBranch>,
}

#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MatchExpr {
    pub kw: Token!["match"],
    pub scrutinee: Result<AnyMatchScrutinee, Missing>,
    pub l: Result<Token!["{"], Missing>,
    pub arms: List<Id<MatchArm>>,
    pub r: Result<Token!["}"], Missing>,
}

#[derive(AnyNode, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnyMatchScrutinee {
    /// `match (x: T) { … }` — the parentheses belong to the match, and may
    /// carry a type ascription.
    Paren(Id<MatchParen>),
    Bare(AnyExpr),
    Bogus(Id<Bogus>),
}

#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MatchParen {
    pub l: Token!["("],
    pub expr: Result<AnyExpr, Missing>,
    pub ascription: Option<Id<TypeAscription>>,
    pub r: Result<Token![")"], Missing>,
}

#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MatchArm {
    pub pattern: Result<AnyPattern, Missing>,
    pub guard: Option<MatchGuard>,
    pub arrow: Result<Token!["=>"], Missing>,
    pub body: Result<AnyExpr, Missing>,
    pub comma: Option<Token![","]>,
}

#[derive(Walk, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MatchGuard {
    pub kw: Token!["if"],
    pub cond: Result<AnyExpr, Missing>,
}

/// `(params) -> R throws E { … }`. Generic parameters on a lambda are an
/// error, kept as [`Bogus`] so they stay in the tree.
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LambdaExpr {
    pub rejected_generics: Option<Id<Bogus>>,
    pub params: Id<ParamList>,
    pub arrow: Result<Token![AnyArrow], Missing>,
    pub ret: Option<AnyType>,
    pub throws: Option<Id<ThrowsClause>>,
    pub body: Result<Id<Block>, Missing>,
}

/// `[a, b]` — commas between elements are optional.
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ArrayLiteral {
    pub l: Token!["["],
    pub elements: List<ArrayElement>,
    pub r: Result<Token!["]"], Missing>,
}

#[derive(Walk, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ArrayElement {
    pub value: AnyExpr,
    pub comma: Option<Token![","]>,
}

/// `map { k: v }` or `{ "k": v }`.
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MapLiteral {
    pub map_kw: Option<Token!["map"]>,
    pub l: Token!["{"],
    pub entries: List<Id<MapEntry>>,
    pub r: Result<Token!["}"], Missing>,
}

#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MapEntry {
    pub key: AnyExpr,
    pub colon: Result<Token![":"], Missing>,
    pub value: Result<AnyExpr, Missing>,
    pub comma: Option<Token![","]>,
}

/// `Point { x: 1, ...rest }`
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ObjectLiteral {
    pub ctor: AnyObjectCtor,
    pub l: Token!["{"],
    pub members: List<AnyObjectMember>,
    pub r: Result<Token!["}"], Missing>,
}

#[derive(AnyNode, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnyObjectCtor {
    Path(Id<PathExpr>),
    Generic(Id<GenericApplyExpr>),
    Bogus(Id<Bogus>),
}

#[derive(AnyNode, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnyObjectMember {
    Field(Id<ObjectField>),
    Spread(Id<SpreadElement>),
    Bogus(Id<Bogus>),
}

/// `key: value`, or the shorthand `key`.
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ObjectField {
    pub key: AnyObjectKey,
    pub value: Option<FieldValue>,
    pub comma: Option<Token![","]>,
}

#[derive(AnyNode, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnyObjectKey {
    Name(Id<DottedName>),
    String(Id<StringLit>),
    Backtick(Id<BacktickString>),
    Bogus(Id<Bogus>),
}

#[derive(Walk, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FieldValue {
    pub colon: Token![":"],
    pub value: Result<AnyExpr, Missing>,
}

/// `...expr`
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SpreadElement {
    pub dots: Token!["..."],
    pub value: Result<AnyExpr, Missing>,
    pub comma: Option<Token![","]>,
}
