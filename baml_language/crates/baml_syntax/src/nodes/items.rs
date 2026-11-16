//! Items, declarations, parameters, generics and attributes.

use super::{
    AnyExpr, AnyType, BacktickString, Block, LetStmt, Name, PathSegment, StringLit, TypeBindingStmt,
};
use crate::{
    AnyNode, Node, Walk,
    token::punct,
    tree::{Bogus, Id, List, Missing, Sep, Sep1, TokenRange},
};

#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SourceFile {
    pub items: List<AnyItem>,
}

#[derive(AnyNode, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnyItem {
    Function(Id<FunctionDef>),
    Class(Id<ClassDef>),
    Enum(Id<EnumDef>),
    Interface(Id<InterfaceDef>),
    ImplementsFor(Id<ImplementsFor>),
    TypeAlias(Id<TypeAliasDef>),
    /// A top-level `let`/`const` global, with its docs.
    Let(Id<LetItem>),
    /// A top-level `let type` parses here and is rejected by lowering.
    TypeBinding(Id<TypeBindingStmt>),
    ClientValue(Id<ClientValueDef>),
    Test(Id<TestDef>),
    Removed(Id<RemovedItem>),
    Bogus(Id<Bogus>),
}

/// A file-scope `let`/`const` binding: the statement, documented. (In a body
/// the statement stands alone; a `///` line there is an error.)
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LetItem {
    pub docs: List<AnyDoc>,
    pub stmt: Id<LetStmt>,
}

// ---------------------------------------------------------------------------
// Functions
// ---------------------------------------------------------------------------

/// `function name<G>(params) -> R throws E { body }`. Also a class method,
/// an interface default method, and an implements-block method.
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FunctionDef {
    pub docs: List<AnyDoc>,
    pub kw: Token!["function"],
    pub name: Result<Id<Name>, Missing>,
    pub generics: Option<Id<GenericParamList>>,
    pub params: Result<Id<ParamList>, Missing>,
    pub ret: Result<Id<ReturnType>, Missing>,
    pub throws: Option<Id<ThrowsClause>>,
    pub body: Result<AnyFunctionBody, Missing>,
}

/// `-> T` (or `=> T`, accepted in its place).
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ReturnType {
    pub arrow: Token![AnyArrow],
    pub ty: Result<AnyType, Missing>,
}

#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ThrowsClause {
    pub kw: Token!["throws"],
    pub ty: Result<AnyType, Missing>,
}

#[derive(AnyNode, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnyFunctionBody {
    Expr(Id<Block>),
    Llm(Id<LlmBody>),
    Bogus(Id<Bogus>),
}

/// `{ client: …  prompt: …  tools: … }`, in any order (duplicates are a
/// lowering diagnostic).
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LlmBody {
    pub l: Token!["{"],
    pub fields: List<AnyLlmField>,
    pub r: Result<Token!["}"], Missing>,
}

#[derive(AnyNode, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnyLlmField {
    Client(Id<ClientField>),
    Prompt(Id<PromptField>),
    Tools(Id<ToolsField>),
    Bogus(Id<Bogus>),
}

#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ClientField {
    pub kw: Token!["client"],
    pub colon: Result<Token![":"], Missing>,
    pub value: Result<AnyExpr, Missing>,
}

#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PromptField {
    pub kw: Token!["prompt"],
    pub colon: Result<Token![":"], Missing>,
    pub value: Result<AnyStringLike, Missing>,
}

#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ToolsField {
    pub kw: Token!["tools"],
    pub colon: Result<Token![":"], Missing>,
    pub value: Result<AnyExpr, Missing>,
}

#[derive(AnyNode, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnyStringLike {
    String(Id<StringLit>),
    Backtick(Id<BacktickString>),
    Bogus(Id<Bogus>),
}

// ---------------------------------------------------------------------------
// Classes, enums, interfaces, implementations
// ---------------------------------------------------------------------------

#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ClassDef {
    pub docs: List<AnyDoc>,
    pub kw: Token!["class"],
    pub name: Result<Id<Name>, Missing>,
    pub generics: Option<Id<GenericParamList>>,
    pub l: Result<Token!["{"], Missing>,
    pub members: List<AnyClassMember>,
    pub r: Result<Token!["}"], Missing>,
}

#[derive(AnyNode, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnyClassMember {
    Field(Id<Field>),
    Method(Id<FunctionDef>),
    Implements(Id<ImplementsBlock>),
    BlockAttr(Id<BlockAttr>),
    Bogus(Id<Bogus>),
}

/// `name: T @attr…,` — the colon may be omitted (`name T`), and the member
/// owns its separator.
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Field {
    pub docs: List<AnyDoc>,
    pub name: Id<Name>,
    pub colon: Option<Token![":"]>,
    pub ty: Result<AnyType, Missing>,
    pub attrs: List<Id<Attribute>>,
    pub sep: Option<Token![MemberSep]>,
}

#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EnumDef {
    pub docs: List<AnyDoc>,
    pub kw: Token!["enum"],
    pub name: Result<Id<Name>, Missing>,
    pub l: Result<Token!["{"], Missing>,
    pub members: List<AnyEnumMember>,
    pub r: Result<Token!["}"], Missing>,
}

#[derive(AnyNode, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnyEnumMember {
    Variant(Id<EnumVariant>),
    BlockAttr(Id<BlockAttr>),
    Bogus(Id<Bogus>),
}

#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EnumVariant {
    pub docs: List<AnyDoc>,
    pub name: Id<Name>,
    pub attrs: List<Id<Attribute>>,
    pub sep: Option<Token![MemberSep]>,
}

#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InterfaceDef {
    pub docs: List<AnyDoc>,
    pub kw: Token!["interface"],
    pub name: Result<Id<Name>, Missing>,
    pub generics: Option<Id<GenericParamList>>,
    pub requires: Option<Id<RequiresClause>>,
    pub l: Result<Token!["{"], Missing>,
    pub members: List<AnyInterfaceMember>,
    pub r: Result<Token!["}"], Missing>,
}

#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RequiresClause {
    pub kw: Token!["requires"],
    pub types: Result<Sep1<AnyType, punct::Comma>, Missing>,
}

#[derive(AnyNode, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnyInterfaceMember {
    MethodSig(Id<MethodSig>),
    Method(Id<FunctionDef>),
    Field(Id<Field>),
    AssocType(Id<AssocTypeDecl>),
    BlockAttr(Id<BlockAttr>),
    Bogus(Id<Bogus>),
}

/// A required method: a signature without a body.
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MethodSig {
    pub docs: List<AnyDoc>,
    pub kw: Token!["function"],
    pub name: Result<Id<Name>, Missing>,
    pub generics: Option<Id<GenericParamList>>,
    pub params: Result<Id<ParamList>, Missing>,
    pub ret: Result<Id<ReturnType>, Missing>,
    pub throws: Option<Id<ThrowsClause>>,
    pub semi: Option<Token![";"]>,
}

/// `type Item extends Bound = Default,` declared in an interface: bound and
/// default both optional.
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AssocTypeDecl {
    pub docs: List<AnyDoc>,
    pub kw: Token!["type"],
    pub name: Result<Id<Name>, Missing>,
    pub bound: Option<AssocBound>,
    pub default: Option<AssocDefault>,
    pub sep: Option<Token![MemberSep]>,
}

#[derive(Walk, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AssocBound {
    pub kw: Token!["extends"],
    pub ty: Result<AnyType, Missing>,
}

#[derive(Walk, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AssocDefault {
    pub eq: Token!["="],
    pub ty: Result<AnyType, Missing>,
}

/// `type Item = T,` defined in an implementation: the binding is required.
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AssocTypeDef {
    pub docs: List<AnyDoc>,
    pub kw: Token!["type"],
    pub name: Result<Id<Name>, Missing>,
    pub eq: Result<Token!["="], Missing>,
    pub ty: Result<AnyType, Missing>,
    pub sep: Option<Token![MemberSep]>,
}

/// `implements I { … }` inside a class.
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ImplementsBlock {
    pub docs: List<AnyDoc>,
    pub kw: Token![ImplementsKw],
    pub target: Result<AnyType, Missing>,
    pub l: Result<Token!["{"], Missing>,
    pub members: List<AnyImplMember>,
    pub r: Result<Token!["}"], Missing>,
}

/// `implements<G> I for T { … }` at the top level.
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ImplementsFor {
    pub docs: List<AnyDoc>,
    pub kw: Token![ImplementsKw],
    pub generics: Option<Id<GenericParamList>>,
    pub target: Result<AnyType, Missing>,
    pub for_kw: Result<Token!["for"], Missing>,
    pub self_ty: Result<AnyType, Missing>,
    pub l: Result<Token!["{"], Missing>,
    pub members: List<AnyImplMember>,
    pub r: Result<Token!["}"], Missing>,
}

#[derive(AnyNode, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnyImplMember {
    Method(Id<FunctionDef>),
    AssocType(Id<AssocTypeDef>),
    FieldLink(Id<InterfaceFieldLink>),
    Field(Id<Field>),
    Bogus(Id<Bogus>),
}

/// `interface_field as class_field,`
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InterfaceFieldLink {
    pub docs: List<AnyDoc>,
    pub iface_field: Id<Name>,
    pub as_kw: Result<Token!["as"], Missing>,
    pub class_field: Result<Id<Name>, Missing>,
    pub sep: Option<Token![MemberSep]>,
}

#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TypeAliasDef {
    pub docs: List<AnyDoc>,
    pub kw: Token!["type"],
    pub name: Result<Id<Name>, Missing>,
    pub eq: Result<Token!["="], Missing>,
    pub ty: Result<AnyType, Missing>,
    pub semi: Option<Token![";"]>,
}

/// `client Name = <expr>;`
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ClientValueDef {
    pub docs: List<AnyDoc>,
    pub kw: Token!["client"],
    pub name: Id<Name>,
    pub eq: Token!["="],
    pub value: Result<AnyExpr, Missing>,
    pub semi: Option<Token![";"]>,
}

/// `test "name" with … { … }` / `testset …` — also a statement inside a
/// testset's block.
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TestDef {
    pub docs: List<AnyDoc>,
    pub kw: Token![TestKw],
    pub name: Result<AnyExpr, Missing>,
    pub with: Option<Id<WithClause>>,
    pub body: Result<Id<Block>, Missing>,
}

/// `with a, b` — on tests, testsets and `spawn`.
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WithClause {
    pub kw: Token!["with"],
    pub items: Sep<AnyExpr, punct::Comma>,
}

/// A removed item form (`client<llm> X { … }`, `retry_policy`,
/// `template_string`, `generator`, `type_builder`, `dynamic class`, a legacy
/// `test X { functions … }`). Kept as token ranges so its diagnostic can point
/// at it and name it; it lowers to nothing. `head` is what sits between the
/// keyword and the name (`<llm>`, the `class` of `dynamic class`), usually
/// empty; `body` is everything after the name.
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RemovedItem {
    pub kw: Token![RemovedKw],
    pub head: TokenRange,
    pub name: Option<Id<Name>>,
    pub body: TokenRange,
}

// ---------------------------------------------------------------------------
// Generics, parameters
// ---------------------------------------------------------------------------

/// Declaration-site `<T, U extends I>`.
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GenericParamList {
    pub l: Token!["<"],
    pub params: Sep<Id<GenericParam>, punct::Comma>,
    pub r: Result<Token![">"], Missing>,
}

#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GenericParam {
    pub name: Id<Name>,
    pub bounds: Option<Id<GenericParamBounds>>,
}

/// `extends A & B` — on a generic parameter or a `let type` binding.
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GenericParamBounds {
    pub kw: Token!["extends"],
    pub first: Result<AnyType, Missing>,
    /// `& T` any number of times; a trailing `&` is an arm whose type is
    /// missing.
    pub rest: List<BoundArm>,
}

#[derive(Walk, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BoundArm {
    pub and: Token!["&"],
    pub ty: Result<AnyType, Missing>,
}

/// Expression-side `foo<T, U>`.
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GenericArgs {
    pub l: Token!["<"],
    pub args: Sep<AnyType, punct::Comma>,
    pub r: Result<Token![">"], Missing>,
}

/// Type-side `Foo<T, Item = U>`.
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TypeArgs {
    pub l: Token!["<"],
    pub args: Sep<AnyTypeArg, punct::Comma>,
    pub r: Result<Token![">"], Missing>,
}

#[derive(AnyNode, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnyTypeArg {
    Type(AnyType),
    Binding(Id<AssocTypeBinding>),
    Bogus(Id<Bogus>),
}

/// `Item = int` inside type arguments.
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AssocTypeBinding {
    pub name: Id<Name>,
    pub eq: Token!["="],
    pub ty: Result<AnyType, Missing>,
}

#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ParamList {
    pub l: Token!["("],
    pub params: Sep<Id<Param>, punct::Comma>,
    pub r: Result<Token![")"], Missing>,
}

/// `name: T = default`. `self` has no type. A parameter written as a bare
/// type keeps the type and misses the name.
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Param {
    pub name: Result<Id<Name>, Missing>,
    pub ascription: Option<Id<TypeAscription>>,
    pub default: Option<Id<DefaultValue>>,
}

/// `: T` — the colon may be omitted where the grammar allows `name T`.
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TypeAscription {
    pub colon: Option<Token![":"]>,
    pub ty: Result<AnyType, Missing>,
}

#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DefaultValue {
    pub eq: Token!["="],
    pub value: Result<AnyExpr, Missing>,
}

// ---------------------------------------------------------------------------
// Attributes
// ---------------------------------------------------------------------------

/// `@name(args)` on a field or variant.
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Attribute {
    pub at: Token!["@"],
    pub path: Result<Id<DottedName>, Missing>,
    pub args: Option<Id<AttrArgs>>,
}

/// `@@name(args)` on a declaration — before it, or inside its body as a
/// member (the two spellings are the same attribute).
/// `@@name(args)`: an attribute of the declaration whose body it is written
/// in — a class, enum or interface. There is no other position for one.
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlockAttr {
    pub at: Token!["@@"],
    pub path: Result<Id<DottedName>, Missing>,
    pub args: Option<Id<AttrArgs>>,
}

/// What precedes a declaration that takes `@@attrs`, in source order: its
/// attributes, its `///` doc lines and its `//baml:` directives. A `///` or
/// `//baml:` line is syntax the declaration owns, as in Rust, where `///` is
/// an attribute; one that precedes no declaration is an error.
#[derive(AnyNode, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnyDoc {
    Doc(Id<DocLine>),
    Directive(Id<Directive>),
    Header(Id<HeaderLine>),
    Bogus(Id<Bogus>),
}

/// A `//#` header line directly above a function: the function's title,
/// which the control-flow visualizer labels its node with. A function takes
/// one; above any other declaration, or twice, it is an error (in a block it
/// is a statement instead, see `HeaderStmt`).
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HeaderLine {
    pub token: Token![HeaderComment],
}

/// One `///` line of a declaration's documentation. Its text is the token's
/// text after the `///`, with one optional leading space; the lines of a
/// declaration join with `\n`.
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DocLine {
    pub token: Token![DocComment],
}

/// A `//baml:<name>` directive on a declaration, or a `/// baml:$trace=path`
/// trace hook: an internal attribute spelled as a comment. The parser
/// classifies it; a line it cannot classify is a `Bogus` with an error.
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Directive {
    pub token: Token![DocToken],
    pub kind: DirectiveKind,
}

/// Which directive a [`Directive`] is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DirectiveKind {
    /// `//baml:mut_self`
    MutSelf,
    /// `//baml:vm`
    Vm,
    /// `//baml:mut_vm`
    MutVm,
    /// `//baml:may_yield`
    MayYield,
    /// `//baml:fallible`
    Fallible,
    /// `//baml:tagged_string`
    TaggedString,
    /// `//baml:trace_passthrough`
    TracePassthrough,
    /// `//baml:local_twin`
    LocalTwin,
    /// `/// baml:$trace=path.to.hook`; the path is
    /// [`DirectiveKind::trace_hook_path`] of the token's text, valid by
    /// construction.
    Trace,
}

impl DirectiveKind {
    /// The directive `//baml:<name>` names, if any.
    pub fn named(name: &str) -> Option<Self> {
        Some(match name.trim_end() {
            "mut_self" => Self::MutSelf,
            "vm" => Self::Vm,
            "mut_vm" => Self::MutVm,
            "may_yield" => Self::MayYield,
            "fallible" => Self::Fallible,
            "tagged_string" => Self::TaggedString,
            "trace_passthrough" => Self::TracePassthrough,
            "local_twin" => Self::LocalTwin,
            _ => return None,
        })
    }

    /// What follows `baml:$trace` in a comment of either spelling
    /// (`/// baml:$trace=…`, `//baml:$trace=…`), if the comment is a trace
    /// hook.
    pub fn trace_hook_value(comment: &str) -> Option<&str> {
        comment
            .trim_start_matches('/')
            .trim()
            .strip_prefix("baml:$trace")
    }

    /// The hook a trace value names: `=a.b.c`, each segment an identifier.
    pub fn trace_hook_path(value: &str) -> Option<&str> {
        let path = value.trim_start().strip_prefix('=')?.trim();
        let is_identifier = |segment: &str| {
            let mut chars = segment.chars();
            chars
                .next()
                .is_some_and(|c| c == '_' || c.is_ascii_alphabetic())
                && chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
        };
        (!path.is_empty() && path.split('.').all(is_identifier)).then_some(path)
    }
}

/// The kind is the parser's reading of the token, not a part of the tree.
impl<A> crate::tree::Walk<A> for DirectiveKind {
    fn walk(&self, _arena: &A, _field: &'static str, _visitor: &mut dyn crate::tree::Visitor) {}
}

/// `a.b.c` where only names are allowed (attribute paths, object keys).
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DottedName {
    pub head: Id<Name>,
    pub segments: List<PathSegment>,
}

#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AttrArgs {
    pub l: Token!["("],
    pub args: Sep<AnyAttrArg, punct::Comma>,
    pub r: Result<Token![")"], Missing>,
}

#[derive(AnyNode, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnyAttrArg {
    String(Id<StringLit>),
    Backtick(Id<BacktickString>),
    Jinja(Id<JinjaArg>),
    /// A bare word: `@alias(foo)`.
    Unquoted(Id<Name>),
    Bogus(Id<Bogus>),
}

/// `{{ … }}` — a Jinja expression argument, kept as unparsed tokens.
#[derive(Node, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct JinjaArg {
    pub l1: Token!["{"],
    pub l2: Token!["{"],
    pub tokens: TokenRange,
    pub r1: Result<Token!["}"], Missing>,
    pub r2: Result<Token!["}"], Missing>,
}
