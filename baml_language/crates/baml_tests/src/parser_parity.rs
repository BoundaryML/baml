//! The two parsers compared, file by file, over every `.baml` source the repo
//! owns (the roots of [`crate::ast_dump`]).
//!
//! Per file, all in-process (no stored baseline):
//!
//! - **errors**: each parser's diagnostics as `(range, text)`, as a multiset;
//! - **shape**: each tree's nodes as `(category, range)`, where a category is
//!   a construct both trees name ([`Category`]) and ranges exclude trivia;
//! - **error presence**: the typed parser may lack errors the rowan parser
//!   reports *inside* a removed item's body (not read), and may not report
//!   errors where the rowan parser has none.
//!
//! Nothing inside a `RemovedItem`'s head/body or a `JinjaArg` body is
//! compared on either side; in a parse with errors, runs of tokens the typed
//! tree does not own are not compared either (recovery skipped them). A
//! reported `Bogus` is *not* opaque: it is a recovery the rowan side must
//! mirror. Known differences are listed in [`PENDING`], which may only shrink:
//! an entry that stops differing, or names no file, fails the test.

use std::{collections::BTreeMap, fmt::Write as _};

use baml_base::FileId;
use baml_compiler_syntax::{SyntaxKind, SyntaxNode, traversal::trimmed_range};
use baml_syntax::{
    nodes::{AnyNodeRef, Arenas, NodeKind},
    token::{TokenIdx, TokenKind, Tokens},
    tree::{AnyNodeId, Missing, TokenRange, Visitor},
};
use text_size::TextRange;

/// Files whose two parses are known to differ, with the reason. Each entry is
/// removed when its layer lands or the difference is ruled acceptable.
pub const PENDING: &[(&str, &str)] = &[
    (
        "projects/broken_syntax/unknown_attributes/unknown_attributes.baml",
        "a `@@` before a declaration is not a position in the new syntax (reported, one `Bogus`); rowan attached it to the function",
    ),
    (
        "projects/broken_syntax/assignment_expr_position/main.baml",
        "an assignment in expression position is a parse error with no node; rowan kept a `BINARY_EXPR` and reported it from the lowering",
    ),
    (
        "projects/broken_syntax/null_handling/null_handling.baml",
        "an assignment in expression position is a parse error with no node; rowan kept a `BINARY_EXPR` and reported it from the lowering",
    ),
    (
        "projects/parser_stress/rust.baml",
        "`///` lines before Rust code document no declaration: an error now that docs are syntax",
    ),
    (
        "projects/broken_syntax/header_in_llm_function/test.baml",
        "the `//#` diagnostic names the rule (a statement) instead of the old exemption",
    ),
    (
        "projects/broken_syntax/type_attributes/signatures.baml",
        "a string in a recovery position is one token, not re-split around its quotes",
    ),
    (
        "projects/broken_syntax/parser_strings/nested_quotes.baml",
        "single-quoted text is `Error` tokens, not re-split around `\\` and `\"`",
    ),
    (
        "projects/broken_syntax/parser_strings/unicode_strings.baml",
        "a non-ASCII name is `Error` tokens on both sides, but the signature now recovers in place",
    ),
    (
        "projects/broken_syntax/pending_greaters_fix/extra_greater.baml",
        "`>>` is two `>` tokens: the extra `>` is a stray token, not an unmatched-`>>` diagnostic",
    ),
    (
        "projects/broken_syntax/type_attributes/nested.baml",
        "`>>` is two `>` tokens: both generic lists close after the attribute error",
    ),
    (
        "projects/diagnostic_errors/test_invalid_contexts/main.baml",
        "a `test` outside a testset is a validation diagnostic (P3.2), not a parse error",
    ),
    (
        "projects/broken_syntax/banned_expressions/banned_expressions.baml",
        "`let a = let b = 42`: the first `let` ends at the second (a statement start) instead of swallowing it as its initialiser; `(x = 5)` and the other assignments in expression position are reported with no node (rowan: a `BINARY_EXPR`)",
    ),
    (
        "projects/broken_syntax/braceless_lambda/braceless_lambda.baml",
        "`x + ;`: the `;` is the statement's, not a garbage right operand inside the binary expression",
    ),
    (
        "projects/broken_syntax/parser_speculative/ambiguous_function.baml",
        "a misspelled LLM field makes an expression body; its stray `:` is one bogus token, not a tagged-template recovery",
    ),
    (
        "projects/broken_syntax/patterns_array_rest_parser_recovery/patterns_array_rest_parser_recovery.baml",
        "`[...let r]`/`[.. : int[]]`: the token that cannot start an element is reported and skipped, not reported as a missing separator",
    ),
    (
        "projects/broken_syntax/is_in_type_position/main.baml",
        "stray text at the top level is one `Bogus` with one diagnostic up to the next item start; rowan reported every token",
    ),
    (
        "projects/broken_syntax/parser_error_recovery/invalid_syntax.baml",
        "stray text at the top level is one `Bogus` with one diagnostic up to the next item start; rowan reported every token",
    ),
    (
        "projects/broken_syntax/parser_error_recovery/partial_input.baml",
        "stray text at the top level is one `Bogus` with one diagnostic up to the next item start; rowan reported every token",
    ),
    (
        "projects/parser_stress/very_invalid.baml",
        "a lexer `Error` or `\\` token cannot start an expression: the statement before it ends there instead of swallowing it",
    ),
];

/// A construct both trees name. Kinds with no counterpart on the other side
/// (token wrappers, layers not yet ported) have none.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Category {
    Function,
    Class,
    Enum,
    Interface,
    TypeAlias,
    ClientValue,
    /// A removed config block the rowan parser still has a node for; a
    /// `RemovedItem` with the same keyword here. Rowan's `ERROR` wrappers are
    /// recovery artifacts, not constructs, and are not compared.
    Removed,
    ImplementsFor,
    ImplementsBlock,
    FieldLink,
    MethodSig,
    AssocType,
    Requires,
    Field,
    Variant,
    Params,
    Param,
    GenericParams,
    GenericParam,
    GenericBounds,
    TypeArgs,
    Attribute,
    BlockAttribute,
    AttributeArgs,
    Jinja,
    Throws,
    /// Every type node; a modifier chain (`T[]?`) counts once, as the rowan
    /// `TYPE_EXPR` does.
    Type,
    FunctionType,
    FunctionTypeParam,
    LlmBody,
    ClientField,
    PromptField,
    ToolsField,
    // ----- bodies
    Block,
    Header,
    Let,
    TypeBinding,
    While,
    WhileLet,
    For,
    Defer,
    Test,
    // ----- expressions
    /// A multi-segment path, or a generic application (`foo<T>` is a rowan
    /// `PATH_EXPR` around the name and its arguments).
    Path,
    Literal,
    Backtick,
    FieldAccess,
    OptionalFieldAccess,
    Upcast,
    /// `(Base as Iface).item` in expression position; in type position the
    /// same typed node is a `Type`.
    Qualified,
    Spec,
    EnvAccess,
    Call,
    OptionalCall,
    Index,
    OptionalIndex,
    TaggedTemplate,
    CallArgs,
    CallArg,
    GenericArgs,
    Binary,
    Is,
    Unary,
    Await,
    Spawn,
    Catch,
    CatchClause,
    CatchArm,
    /// `throw` as an expression, or a statement with its `;` left out.
    Throw,
    Return,
    Break,
    Continue,
    Paren,
    If,
    IfLet,
    Match,
    MatchArm,
    Lambda,
    Array,
    Map,
    Object,
    /// An object field or a map entry (one rowan kind serves both).
    ObjectField,
    Spread,
    // ----- templates
    Interpolation,
    ForOpen,
    IfOpen,
    ElseIf,
    Else,
    EndTag,
    // ----- patterns
    UnionPattern,
    BindingPattern,
    WildcardPattern,
    DestructurePattern,
    FieldPattern,
    ArrayPattern,
    TypePattern,
    ParenPattern,
}

/// `parent` disambiguates rowan's `FUNCTION_TYPE_PARAM`: it also wraps the
/// operand of a parenthesized type (the parser decides after parsing), where
/// the typed tree has a `ParenType` and no parameter. A `LET_STMT` under a
/// `FOR_EXPR` is a header part, not a statement, on both sides.
fn rowan_category(kind: SyntaxKind, parent: Option<SyntaxKind>) -> Option<Category> {
    use SyntaxKind as K;
    Some(match kind {
        // An interpolation's body is a `BLOCK_EXPR` here and inline parts
        // there; a `${for}` tag's pattern is a `LET_STMT` here and a header
        // part there.
        K::BLOCK_EXPR if parent == Some(K::BACKTICK_INTERPOLATION) => return None,
        K::BLOCK_EXPR => Category::Block,
        K::HEADER_COMMENT => Category::Header,
        K::LET_STMT if parent == Some(K::FOR_EXPR) || parent == Some(K::BACKTICK_FOR_OPEN) => {
            return None;
        }
        K::BACKTICK_INTERPOLATION => Category::Interpolation,
        K::BACKTICK_FOR_OPEN => Category::ForOpen,
        K::BACKTICK_IF_OPEN => Category::IfOpen,
        K::BACKTICK_ELSE_IF => Category::ElseIf,
        K::BACKTICK_ELSE => Category::Else,
        K::BACKTICK_ENDFOR | K::BACKTICK_ENDIF => Category::EndTag,
        K::LET_STMT => Category::Let,
        K::TYPE_BINDING_STMT => Category::TypeBinding,
        K::WHILE_STMT => Category::While,
        K::WHILE_LET_STMT => Category::WhileLet,
        K::FOR_EXPR => Category::For,
        K::DEFER_STMT => Category::Defer,
        K::TEST_EXPR_DEF | K::TESTSET_DEF => Category::Test,
        K::PATH_EXPR => Category::Path,
        // A string literal type is a `STRING_LITERAL` under its `TYPE_EXPR`;
        // the typed `LiteralType` is the `Type` alone.
        K::STRING_LITERAL
        | K::RAW_STRING_LITERAL
        | K::BYTE_STRING_LITERAL
        | K::DEPRECATED_HASH_STRING_LITERAL
            if parent == Some(K::TYPE_EXPR) =>
        {
            return None;
        }
        K::STRING_LITERAL
        | K::RAW_STRING_LITERAL
        | K::BYTE_STRING_LITERAL
        | K::DEPRECATED_HASH_STRING_LITERAL => Category::Literal,
        K::BACKTICK_STRING_LITERAL => Category::Backtick,
        K::FIELD_ACCESS_EXPR => Category::FieldAccess,
        K::OPTIONAL_FIELD_ACCESS_EXPR => Category::OptionalFieldAccess,
        K::UPCAST_EXPR => Category::Upcast,
        K::QUALIFIED_PATH_EXPR => Category::Qualified,
        K::SPEC_EXPR => Category::Spec,
        K::ENV_ACCESS_EXPR => Category::EnvAccess,
        K::CALL_EXPR => Category::Call,
        K::OPTIONAL_CALL_EXPR => Category::OptionalCall,
        K::INDEX_EXPR => Category::Index,
        K::OPTIONAL_INDEX_EXPR => Category::OptionalIndex,
        K::TAGGED_TEMPLATE_EXPR => Category::TaggedTemplate,
        K::CALL_ARGS => Category::CallArgs,
        K::CALL_ARG => Category::CallArg,
        K::GENERIC_ARGS => Category::GenericArgs,
        K::BINARY_EXPR => Category::Binary,
        K::IS_EXPR => Category::Is,
        K::UNARY_EXPR => Category::Unary,
        K::AWAIT_EXPR => Category::Await,
        K::SPAWN_EXPR => Category::Spawn,
        K::CATCH_EXPR => Category::Catch,
        K::CATCH_CLAUSE => Category::CatchClause,
        K::CATCH_ARM => Category::CatchArm,
        K::THROW_EXPR => Category::Throw,
        K::RETURN_EXPR | K::RETURN_STMT => Category::Return,
        K::BREAK_EXPR | K::BREAK_STMT => Category::Break,
        K::CONTINUE_EXPR | K::CONTINUE_STMT => Category::Continue,
        K::PAREN_EXPR => Category::Paren,
        K::IF_EXPR => Category::If,
        K::IF_LET_EXPR => Category::IfLet,
        K::MATCH_EXPR => Category::Match,
        K::MATCH_ARM => Category::MatchArm,
        K::LAMBDA_EXPR => Category::Lambda,
        K::ARRAY_LITERAL => Category::Array,
        K::MAP_LITERAL => Category::Map,
        K::OBJECT_LITERAL => Category::Object,
        K::OBJECT_FIELD => Category::ObjectField,
        K::SPREAD_ELEMENT => Category::Spread,
        K::UNION_PATTERN => Category::UnionPattern,
        K::BINDING_PATTERN => Category::BindingPattern,
        K::WILDCARD_PATTERN => Category::WildcardPattern,
        K::DESTRUCTURE_PATTERN => Category::DestructurePattern,
        K::FIELD_PATTERN => Category::FieldPattern,
        K::ARRAY_PATTERN => Category::ArrayPattern,
        K::ARRAY_PATTERN_ELEMENT if parent == Some(K::ARRAY_PATTERN) => return None,
        K::TYPE_PATTERN => Category::TypePattern,
        K::PAREN_PATTERN => Category::ParenPattern,
        K::FUNCTION_DEF => Category::Function,
        K::CLASS_DEF => Category::Class,
        K::ENUM_DEF => Category::Enum,
        K::INTERFACE_DEF => Category::Interface,
        K::TYPE_ALIAS_DEF => Category::TypeAlias,
        K::CLIENT_VALUE_DEF => Category::ClientValue,
        K::CLIENT_DEF | K::GENERATOR_DEF | K::RETRY_POLICY_DEF | K::TEMPLATE_STRING_DEF => {
            Category::Removed
        }
        K::IMPLEMENTS_FOR => Category::ImplementsFor,
        K::IMPLEMENTS_BLOCK => Category::ImplementsBlock,
        K::INTERFACE_FIELD_LINK => Category::FieldLink,
        K::METHOD_SIG => Category::MethodSig,
        K::ASSOCIATED_TYPE_DECL => Category::AssocType,
        K::REQUIRES_CLAUSE => Category::Requires,
        K::FIELD => Category::Field,
        K::ENUM_VARIANT => Category::Variant,
        K::PARAMETER_LIST => Category::Params,
        K::PARAMETER => Category::Param,
        K::GENERIC_PARAM_LIST => Category::GenericParams,
        K::GENERIC_PARAM => Category::GenericParam,
        K::GENERIC_PARAM_BOUNDS => Category::GenericBounds,
        K::TYPE_ARGS => Category::TypeArgs,
        K::ATTRIBUTE => Category::Attribute,
        K::BLOCK_ATTRIBUTE => Category::BlockAttribute,
        K::ATTRIBUTE_ARGS => Category::AttributeArgs,
        K::EXPR => Category::Jinja,
        K::THROWS_CLAUSE => Category::Throws,
        K::TYPE_EXPR => Category::Type,
        K::FUNCTION_TYPE => Category::FunctionType,
        K::FUNCTION_TYPE_PARAM if parent == Some(K::FUNCTION_TYPE) => Category::FunctionTypeParam,
        K::FUNCTION_TYPE_PARAM => return None,
        K::LLM_FUNCTION_BODY => Category::LlmBody,
        K::CLIENT_FIELD => Category::ClientField,
        K::PROMPT_FIELD => Category::PromptField,
        K::TOOLS_FIELD => Category::ToolsField,
        _ => return None,
    })
}

/// The categories a typed node contributes: a function type is both a
/// `TYPE_EXPR` and a `FUNCTION_TYPE` on the rowan side. `in_type` says
/// whether the node sits in type position, for the one node that serves both
/// (`QualifiedPath`); `multi_segment` whether a path has segments (a lone
/// name is a bare token on the rowan side).
fn typed_categories(kind: NodeKind, in_type: bool, multi_segment: bool) -> &'static [Category] {
    use NodeKind as K;
    match kind {
        K::Block => &[Category::Block],
        K::HeaderStmt => &[Category::Header],
        K::LetStmt => &[Category::Let],
        // An assignment was a `BINARY_EXPR` with an assignment operator.
        K::AssignStmt => &[Category::Binary],
        K::TypeBindingStmt => &[Category::TypeBinding],
        K::WhileStmt => &[Category::While],
        K::WhileLetStmt => &[Category::WhileLet],
        K::ForStmt => &[Category::For],
        K::DeferStmt => &[Category::Defer],
        K::TestDef => &[Category::Test],
        K::PathExpr if multi_segment => &[Category::Path],
        K::PathExpr => &[],
        K::GenericApplyExpr => &[Category::Path],
        K::StringLit => &[Category::Literal],
        K::BacktickString => &[Category::Backtick],
        K::FieldAccessExpr => &[Category::FieldAccess],
        K::OptionalFieldAccessExpr => &[Category::OptionalFieldAccess],
        K::UpcastExpr => &[Category::Upcast],
        K::QualifiedPath if in_type => &[Category::Type],
        K::QualifiedPath => &[Category::Qualified],
        K::SpecExpr => &[Category::Spec],
        K::EnvAccessExpr => &[Category::EnvAccess],
        K::CallExpr => &[Category::Call],
        K::OptionalCallExpr => &[Category::OptionalCall],
        K::IndexExpr => &[Category::Index],
        K::OptionalIndexExpr => &[Category::OptionalIndex],
        K::TaggedTemplateExpr => &[Category::TaggedTemplate],
        K::ArgList => &[Category::CallArgs],
        K::Arg => &[Category::CallArg],
        K::GenericArgs => &[Category::GenericArgs],
        K::BinaryExpr => &[Category::Binary],
        K::IsExpr => &[Category::Is],
        K::UnaryExpr => &[Category::Unary],
        K::AwaitExpr => &[Category::Await],
        K::SpawnExpr => &[Category::Spawn],
        K::CatchExpr => &[Category::Catch],
        K::CatchClause => &[Category::CatchClause],
        K::CatchArm => &[Category::CatchArm],
        K::ThrowExpr => &[Category::Throw],
        K::ReturnExpr => &[Category::Return],
        K::BreakExpr => &[Category::Break],
        K::ContinueExpr => &[Category::Continue],
        K::ParenExpr => &[Category::Paren],
        K::IfExpr => &[Category::If],
        K::IfLetExpr => &[Category::IfLet],
        K::MatchExpr => &[Category::Match],
        K::MatchArm => &[Category::MatchArm],
        K::LambdaExpr => &[Category::Lambda],
        K::ArrayLiteral => &[Category::Array],
        K::MapLiteral => &[Category::Map],
        K::ObjectLiteral => &[Category::Object],
        K::ObjectField | K::MapEntry => &[Category::ObjectField],
        K::SpreadElement => &[Category::Spread],
        K::TemplateInterp => &[Category::Interpolation],
        K::TemplateForOpen => &[Category::ForOpen],
        K::TemplateIfOpen => &[Category::IfOpen],
        K::TemplateElif => &[Category::ElseIf],
        K::TemplateElse => &[Category::Else],
        K::TemplateEndTag => &[Category::EndTag],
        K::UnionPattern => &[Category::UnionPattern],
        K::BindingPattern => &[Category::BindingPattern],
        K::WildcardPattern => &[Category::WildcardPattern],
        K::DestructurePattern => &[Category::DestructurePattern],
        K::FieldPattern => &[Category::FieldPattern],
        K::ArrayPattern => &[Category::ArrayPattern],
        K::TypePattern => &[Category::TypePattern],
        K::ParenPattern => &[Category::ParenPattern],
        K::FunctionDef => &[Category::Function],
        K::ClassDef => &[Category::Class],
        K::EnumDef => &[Category::Enum],
        K::InterfaceDef => &[Category::Interface],
        K::TypeAliasDef => &[Category::TypeAlias],
        K::ClientValueDef => &[Category::ClientValue],
        K::RemovedItem => &[Category::Removed],
        K::ImplementsFor => &[Category::ImplementsFor],
        K::ImplementsBlock => &[Category::ImplementsBlock],
        K::InterfaceFieldLink => &[Category::FieldLink],
        K::MethodSig => &[Category::MethodSig],
        K::AssocTypeDecl | K::AssocTypeDef | K::AssocTypeBinding => &[Category::AssocType],
        K::RequiresClause => &[Category::Requires],
        K::Field => &[Category::Field],
        K::EnumVariant => &[Category::Variant],
        K::ParamList => &[Category::Params],
        K::Param => &[Category::Param],
        K::GenericParamList => &[Category::GenericParams],
        K::GenericParam => &[Category::GenericParam],
        K::GenericParamBounds => &[Category::GenericBounds],
        K::TypeArgs => &[Category::TypeArgs],
        K::Attribute => &[Category::Attribute],
        K::BlockAttr => &[Category::BlockAttribute],
        K::AttrArgs => &[Category::AttributeArgs],
        K::JinjaArg => &[Category::Jinja],
        K::ThrowsClause => &[Category::Throws],
        K::PathType
        | K::LiteralType
        | K::ParenType
        | K::OptionalType
        | K::ArrayType
        | K::UnionType => &[Category::Type],
        K::FunctionType => &[Category::Type, Category::FunctionType],
        K::FunctionTypeParam => &[Category::FunctionTypeParam],
        K::LlmBody => &[Category::LlmBody],
        K::ClientField => &[Category::ClientField],
        K::PromptField => &[Category::PromptField],
        K::ToolsField => &[Category::ToolsField],
        _ => &[],
    }
}

/// Kinds whose member separator (`,`/`;`) the typed node owns and the rowan
/// node leaves outside (an assignment was a `BINARY_EXPR`, its `;` the
/// block's).
fn owns_separator(kind: NodeKind) -> bool {
    matches!(
        kind,
        NodeKind::AssignStmt
            | NodeKind::Field
            | NodeKind::EnumVariant
            | NodeKind::AssocTypeDecl
            | NodeKind::AssocTypeDef
            | NodeKind::InterfaceFieldLink
            | NodeKind::MethodSig
            | NodeKind::ObjectField
            | NodeKind::MapEntry
            | NodeKind::SpreadElement
    )
}

/// Rowan statement kinds that own a trailing `;` the typed expression they
/// correspond to does not (the `;` is the `ExprStmt`'s).
fn rowan_owns_semicolon(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::RETURN_STMT | SyntaxKind::BREAK_STMT | SyntaxKind::CONTINUE_STMT
    )
}

/// Parents under which a child `AnyType` sits in type position; everything
/// else holding one is an expression context. Only `LambdaExpr` mixes the
/// two, by field.
fn child_in_type_position(parent: NodeKind, field: &str) -> bool {
    use NodeKind as K;
    matches!(
        parent,
        K::PathType
            | K::LiteralType
            | K::ParenType
            | K::FunctionType
            | K::FunctionTypeParam
            | K::OptionalType
            | K::ArrayType
            | K::UnionType
            | K::QualifiedPath
            | K::TypeArgs
            | K::AssocTypeBinding
            | K::GenericArgs
            | K::TypeAscription
            | K::ReturnType
            | K::ThrowsClause
            | K::Field
            | K::RequiresClause
            | K::AssocTypeDecl
            | K::AssocTypeDef
            | K::ImplementsBlock
            | K::ImplementsFor
            | K::TypeAliasDef
            | K::GenericParamBounds
            | K::TypePattern
    ) || (parent == K::LambdaExpr && field == "ret")
}

/// One comparable fact: a byte range and what is there.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Entry {
    range: (u32, u32),
    text: String,
}

impl Entry {
    fn new(range: TextRange, text: impl Into<String>) -> Self {
        Self {
            range: (range.start().into(), range.end().into()),
            text: text.into(),
        }
    }

    /// Within one of `opaque`. An empty entry at a range's end is not inside
    /// it: an EOF caret after a placeholder is about what follows.
    fn inside(&self, opaque: &[TextRange]) -> bool {
        let (start, end) = self.range;
        opaque.iter().any(|r| {
            let (rs, re) = (u32::from(r.start()), u32::from(r.end()));
            !r.is_empty() && rs <= start && end <= re && !(start == end && start == re)
        })
    }
}

type Multiset = BTreeMap<Entry, usize>;

fn multiset(entries: impl IntoIterator<Item = Entry>) -> Multiset {
    let mut set = Multiset::new();
    for entry in entries {
        *set.entry(entry).or_default() += 1;
    }
    set
}

/// Collects the typed tree's nodes with their spans, and the ranges nothing
/// is compared inside.
struct TypedWalk<'a> {
    arenas: &'a Arenas,
    tokens: &'a Tokens,
    /// Each node with its kind, span, whether it is in type position and
    /// whether it is a multi-segment path.
    nodes: Vec<(AnyNodeId, NodeKind, TokenRange, bool, bool)>,
    /// The start a statement-level pattern is compared from: its statement's
    /// introducer, which the rowan pattern node includes.
    extend_start: BTreeMap<AnyNodeId, TokenIdx>,
    /// The end a template `${else if}`/`${else}` is compared to: its tag's
    /// `}`, the rowan node being the tag alone while the typed node holds the
    /// parts it guards.
    trim_end: BTreeMap<AnyNodeId, TokenIdx>,
    /// `inner` operands of a modifier (part of the outer chain's `TYPE_EXPR`)
    /// and removed items the rowan tree has no node for.
    suppressed: Vec<AnyNodeId>,
    in_modifier: bool,
    /// Ranges of the typed parser's `RemovedFeature` diagnostics (the removed
    /// form's keyword): a `Bogus` starting at one is removed syntax, read on
    /// neither side.
    removed: Vec<TextRange>,
    /// Removed-item heads and bodies, removed-syntax `Bogus`, jinja bodies.
    opaque: Vec<TextRange>,
    /// Nodes to visit, each with whether it is in type position.
    pending: Vec<(AnyNodeId, bool)>,
    owned: Vec<TokenIdx>,
    /// The node being walked, for its children's positions.
    current: Option<NodeKind>,
}

impl Visitor for TypedWalk<'_> {
    fn node(&mut self, field: &'static str, id: AnyNodeId) {
        if self.in_modifier && field == "inner" {
            self.suppressed.push(id);
        }
        let parent = self
            .current
            .unwrap_or_else(|| unreachable!("visited under a node"));
        self.pending
            .push((id, child_in_type_position(parent, field)));
    }
    fn token(&mut self, _field: &'static str, token: TokenIdx) {
        self.owned.push(token);
    }
    fn range(&mut self, _field: &'static str, range: TokenRange) {
        self.owned.extend(range.iter());
    }
    fn missing(&mut self, _field: &'static str, _missing: Missing) {}
}

impl TypedWalk<'_> {
    fn text_range(&self, range: TokenRange) -> TextRange {
        if range.is_empty() {
            return TextRange::empty(0.into());
        }
        TextRange::new(
            self.tokens.range(range.start()).start(),
            self.tokens.range(TokenIdx(range.end().0 - 1)).end(),
        )
    }

    fn run(&mut self, root: AnyNodeId) {
        self.pending.push((root, false));
        while let Some((id, in_type)) = self.pending.pop() {
            let kind = Arenas::kind(id);
            let span = self.arenas.span(id);
            let mut multi_segment = false;
            match self.arenas.node(id) {
                AnyNodeRef::PathExpr(path) => multi_segment = !path.segments.is_empty(),
                // A destructure's class path is bare tokens on the rowan side.
                AnyNodeRef::DestructurePattern(pattern) => {
                    self.suppressed.push(AnyNodeId::of(pattern.path));
                }
                // A C-style `for` initialiser is a header part, as rowan's
                // `LET_STMT` under `FOR_EXPR` is.
                AnyNodeRef::CForHeader(header) => {
                    if let Some(baml_syntax::nodes::AnyForInit::Let(init)) = header.init {
                        self.suppressed.push(AnyNodeId::of(init));
                    }
                }
                AnyNodeRef::LetStmt(stmt) => {
                    if let Ok(pattern) = stmt.pattern {
                        self.extend_pattern(pattern, stmt.introducer.idx);
                    }
                }
                AnyNodeRef::IfLetExpr(expr) => {
                    if let Ok(pattern) = expr.pattern {
                        self.extend_pattern(pattern, expr.introducer.idx);
                    }
                }
                AnyNodeRef::WhileLetStmt(stmt) => {
                    if let Ok(pattern) = stmt.pattern {
                        self.extend_pattern(pattern, stmt.introducer.idx);
                    }
                }
                AnyNodeRef::ForInHeader(header) => {
                    if let Ok(pattern) = header.pattern {
                        self.extend_pattern(pattern, header.introducer.idx);
                    }
                }
                AnyNodeRef::TemplateElif(elif) => {
                    if let Ok(close) = elif.close {
                        self.trim_end.insert(id, TokenIdx(close.idx.0 + 1));
                    }
                }
                AnyNodeRef::TemplateElse(part) => {
                    if let Ok(close) = part.close {
                        self.trim_end.insert(id, TokenIdx(close.idx.0 + 1));
                    }
                }
                AnyNodeRef::RemovedItem(item) => {
                    self.opaque.push(self.text_range(item.head));
                    self.opaque.push(self.text_range(item.body));
                    if !matches!(
                        self.tokens.kind(item.kw.idx),
                        TokenKind::Client
                            | TokenKind::Generator
                            | TokenKind::RetryPolicy
                            | TokenKind::TemplateString
                    ) {
                        self.suppressed.push(id);
                    }
                }
                AnyNodeRef::JinjaArg(arg) => self.opaque.push(self.text_range(arg.tokens)),
                AnyNodeRef::Bogus(_) => {
                    let range = self.text_range(span);
                    if self.removed.iter().any(|r| r.start() == range.start()) {
                        self.opaque.push(range);
                    }
                }
                _ => {}
            }
            self.nodes.push((id, kind, span, in_type, multi_segment));
            self.in_modifier = matches!(kind, NodeKind::OptionalType | NodeKind::ArrayType);
            self.current = Some(kind);
            self.arenas.walk_node(id, self);
        }
        self.owned.sort_unstable();
    }

    /// A statement's `let`/`const` is the statement's, but the rowan pattern
    /// node under the statement includes it: compare the typed pattern from
    /// the introducer. A top-level array pattern is the exception on both
    /// sides (the introducer sits outside it).
    fn extend_pattern(&mut self, pattern: baml_syntax::nodes::AnyPattern, introducer: TokenIdx) {
        use baml_syntax::nodes::AnyPattern as P;
        let id = match pattern {
            P::Binding(id) => AnyNodeId::of(id),
            P::Wildcard(id) => AnyNodeId::of(id),
            P::Destructure(id) => AnyNodeId::of(id),
            // The union and its first alternative both begin at the
            // introducer on the rowan side.
            P::Union(id) => {
                self.extend_pattern(self.arenas.get(id).first, introducer);
                AnyNodeId::of(id)
            }
            P::Array(_) | P::Type(_) | P::Paren(_) | P::Bogus(_) => return,
        };
        self.extend_start.insert(id, introducer);
    }

    /// Runs of significant tokens no node owns: recovery skipped them.
    fn gaps(&self) -> Vec<TextRange> {
        let mut gaps = Vec::new();
        let mut run: Option<TextRange> = None;
        for idx in self.tokens.iter() {
            // Doc lines are trivia to the rowan parser: never a gap it sees.
            if self.tokens.kind(idx).is_trivia() || self.tokens.kind(idx).is_doc() {
                continue;
            }
            let range = self.tokens.range(idx);
            if self.owned.binary_search(&idx).is_ok() {
                gaps.extend(run.take());
            } else {
                run = Some(run.map_or(range, |r| r.cover(range)));
            }
        }
        gaps.extend(run);
        gaps
    }

    /// A node's span without the `///`/`//baml:` lines it owns (trivia on the
    /// rowan side) and without an owned member separator and the trivia
    /// before it, so it matches the rowan node's trimmed range.
    fn comparable_span(&self, kind: NodeKind, span: TokenRange) -> TokenRange {
        if span.is_empty() {
            return span;
        }
        let mut start = span.start().0;
        if self.tokens.kind(TokenIdx(start)).is_doc() {
            while start < span.end().0 && {
                let kind = self.tokens.kind(TokenIdx(start));
                kind.is_doc() || kind.is_trivia()
            } {
                start += 1;
            }
        }
        let span = TokenRange::new(TokenIdx(start), span.end());
        if !owns_separator(kind) || span.is_empty() {
            return span;
        }
        let mut end = span.end().0;
        if matches!(
            self.tokens.kind(TokenIdx(end - 1)),
            TokenKind::Comma | TokenKind::Semicolon
        ) {
            end -= 1;
            while end > span.start().0 && self.tokens.kind(TokenIdx(end - 1)).is_trivia() {
                end -= 1;
            }
        }
        TokenRange::new(span.start(), TokenIdx(end))
    }

    fn entries(&self) -> Vec<Entry> {
        self.nodes
            .iter()
            .filter(|(id, ..)| !self.suppressed.contains(id))
            .flat_map(|&(id, kind, span, in_type, multi_segment)| {
                let start = self.extend_start.get(&id).copied().unwrap_or(span.start());
                let end = self.trim_end.get(&id).copied().unwrap_or(span.end());
                let span = TokenRange::new(start, end);
                let range = self.text_range(self.comparable_span(kind, span));
                typed_categories(kind, in_type, multi_segment)
                    .iter()
                    .map(move |category| Entry::new(range, format!("{category:?}")))
            })
            .collect()
    }
}

fn rowan_error_entry(error: &baml_compiler_diagnostics::ParseError) -> Entry {
    use baml_compiler_diagnostics::ParseError as E;
    match error {
        E::UnexpectedToken {
            expected,
            found,
            span,
        } => Entry::new(span.range, format!("expected {expected}, found {found}")),
        E::UnexpectedEof { expected, span } => {
            Entry::new(span.range, format!("expected {expected}, found EOF"))
        }
        E::InvalidSyntax { message, span } => Entry::new(span.range, message.clone()),
        E::AmbiguousUnion { span, groupings } => {
            Entry::new(span.range, format!("ambiguous union {groupings:?}"))
        }
        E::RemovedFeature { message, span } => {
            Entry::new(span.range, format!("removed: {message}"))
        }
    }
}

/// `None` for an error the rowan parser never reported (its lowering did,
/// and that lowering is gone): the formatter's parser has no counterpart.
fn typed_error_entry(error: &baml_parser::ParseError) -> Option<Entry> {
    use baml_parser::ParseError as E;
    Some(match error {
        E::UnexpectedToken {
            expected,
            found,
            range,
        } => Entry::new(*range, format!("expected {expected}, found {found}")),
        E::UnexpectedEof { expected, range } => {
            Entry::new(*range, format!("expected {expected}, found EOF"))
        }
        E::InvalidSyntax { message, range } => Entry::new(*range, message.clone()),
        E::AmbiguousUnion { range, groupings } => {
            Entry::new(*range, format!("ambiguous union {groupings:?}"))
        }
        E::RemovedFeature { message, range } => Entry::new(*range, format!("removed: {message}")),
        E::InvalidStringEscape { .. } | E::InvalidDirective { .. } => return None,
    })
}

/// Both parses of `text`, reduced to comparable facts. `Ok` when they agree;
/// `Err` reports every difference.
pub fn compare(text: &str) -> Result<(), String> {
    let typed = baml_parser::parse(text);
    let mut walk = TypedWalk {
        arenas: typed.tree.arenas(),
        tokens: typed.tree.tokens(),
        nodes: Vec::new(),
        extend_start: BTreeMap::new(),
        trim_end: BTreeMap::new(),
        suppressed: Vec::new(),
        in_modifier: false,
        removed: typed
            .errors
            .iter()
            .filter_map(|error| match error {
                baml_parser::ParseError::RemovedFeature { range, .. } => Some(*range),
                _ => None,
            })
            .collect(),
        opaque: Vec::new(),
        pending: Vec::new(),
        owned: Vec::new(),
        current: None,
    };
    walk.run(typed.tree.root_id());
    // What the typed parser does not read: the only places it may lack a
    // diagnostic the rowan parser has.
    let unread = walk.opaque.clone();
    let mut opaque = unread.clone();
    if !typed.errors.is_empty() {
        opaque.extend(walk.gaps());
    }
    let keep = |entry: &Entry| !entry.inside(&opaque);

    let typed_shape = multiset(walk.entries().into_iter().filter(keep));
    let typed_errors = multiset(
        typed
            .errors
            .iter()
            .filter_map(typed_error_entry)
            .filter(keep),
    );

    let tokens = baml_compiler_lexer::lex_lossless(text, FileId::new(0));
    let (green, errors) = baml_compiler_parser::parse_file(&tokens);
    let root = SyntaxNode::new_root(green);
    let rowan_nodes: Vec<(SyntaxKind, Option<SyntaxKind>, TextRange)> = root
        .descendants()
        .map(|node| {
            let parent = node.parent().map(|parent| parent.kind());
            let mut range = trimmed_range(&node);
            if rowan_owns_semicolon(node.kind())
                && let Some(end) = significant_end_before_semicolon(&node)
            {
                range = TextRange::new(range.start(), end);
            }
            (node.kind(), parent, range)
        })
        .collect();
    let rowan_shape = multiset(
        rowan_nodes
            .iter()
            .filter_map(|&(kind, parent, range)| {
                let category = rowan_category(kind, parent)?;
                (!range.is_empty()).then(|| Entry::new(range, format!("{category:?}")))
            })
            .filter(keep),
    );
    let rowan_error_entries: Vec<Entry> = errors.iter().map(rowan_error_entry).collect();
    let rowan_errors = multiset(
        rowan_error_entries
            .iter()
            .filter(|&entry| keep(entry))
            .cloned(),
    );

    let mut report = String::new();
    diff(&mut report, "shape", &rowan_shape, &typed_shape);
    diff(&mut report, "errors", &rowan_errors, &typed_errors);

    if typed.errors.is_empty() {
        for entry in rowan_error_entries
            .iter()
            .filter(|entry| !entry.inside(&unread))
        {
            let _ = writeln!(
                report,
                "  errors: rowan reports where the typed tree is read and error-free  {:?} {}",
                entry.range, entry.text
            );
        }
    }
    if errors.is_empty() {
        for entry in typed.errors.iter().filter_map(typed_error_entry) {
            let _ = writeln!(
                report,
                "  errors: typed reports, rowan has none  {:?} {}",
                entry.range, entry.text
            );
        }
    }

    if report.is_empty() {
        Ok(())
    } else {
        Err(report)
    }
}

/// The end of the last significant token of `node` before its trailing `;`,
/// if it has one.
fn significant_end_before_semicolon(node: &SyntaxNode) -> Option<text_size::TextSize> {
    let significant: Vec<_> = node
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| !token.kind().is_trivia())
        .collect();
    let (last, before) = significant.split_last()?;
    (last.kind() == SyntaxKind::SEMICOLON).then(|| {
        before
            .last()
            .map_or(node.text_range().start(), |token| token.text_range().end())
    })
}

fn diff(report: &mut String, what: &str, rowan: &Multiset, typed: &Multiset) {
    for (entry, &n) in rowan {
        let m = typed.get(entry).copied().unwrap_or(0);
        if n > m {
            let _ = writeln!(
                report,
                "  {what}: only rowan ×{}  {:?} {}",
                n - m,
                entry.range,
                entry.text
            );
        }
    }
    for (entry, &m) in typed {
        let n = rowan.get(entry).copied().unwrap_or(0);
        if m > n {
            let _ = writeln!(
                report,
                "  {what}: only typed ×{}  {:?} {}",
                m - n,
                entry.range,
                entry.text
            );
        }
    }
}

#[test]
fn parser_parity() {
    let mut failures = String::new();
    let mut unexpected_passes = Vec::new();
    let mut seen_pending = Vec::new();
    let mut compared = 0usize;
    for (label, path) in crate::ast_dump::source_files() {
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
        compared += 1;
        let pending = PENDING.iter().any(|(file, _)| *file == label);
        if pending {
            seen_pending.push(label.clone());
        }
        match compare(&text) {
            Ok(()) if pending => unexpected_passes.push(label),
            Ok(()) => {}
            Err(_) if pending => {}
            Err(report) => {
                let _ = writeln!(failures, "{label}:\n{report}");
            }
        }
    }
    assert!(
        compared > 500,
        "expected the whole corpus, compared {compared} files"
    );
    let unmatched: Vec<&str> = PENDING
        .iter()
        .map(|(file, _)| *file)
        .filter(|file| !seen_pending.iter().any(|seen| seen == file))
        .collect();
    assert!(
        unmatched.is_empty(),
        "PENDING names files that do not exist: {unmatched:#?}"
    );
    if !unexpected_passes.is_empty() {
        let _ = writeln!(
            failures,
            "these files now parse identically; remove them from PENDING: {unexpected_passes:#?}"
        );
    }
    assert!(
        failures.is_empty(),
        "parser parity differences:\n{failures}"
    );
}
