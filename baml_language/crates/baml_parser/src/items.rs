//! Declarations: the file, items, their members, signatures, parameters,
//! generics and attributes. Section order follows the rowan parser so the
//! two read side by side.

use baml_syntax::{
    Token,
    nodes::{
        AnyAttrArg, AnyClassMember, AnyEnumMember, AnyFunctionBody, AnyImplMember,
        AnyInterfaceMember, AnyItem, AnyLlmField, AnyStringLike, AssocBound, AssocDefault,
        AssocTypeDecl, AssocTypeDef, AttrArgs, Attribute, BlockAttr, BoundArm, ClassDef,
        ClientField, ClientValueDef, DefaultValue, DottedName, EnumDef, EnumVariant, Field,
        FunctionDef, GenericParam, GenericParamBounds, GenericParamList, ImplementsBlock,
        ImplementsFor, InterfaceDef, InterfaceFieldLink, JinjaArg, LlmBody, MethodSig, Name, Param,
        ParamList, PathSegment, PromptField, RemovedItem, RequiresClause, ReturnType, SourceFile,
        StringLit, ThrowsClause, ToolsField, TypeAliasDef, TypeAscription,
    },
    token::{
        Ident, KindSet, TokenIdx, TokenKind, TokenSet as _, ctx, kw, punct,
        sets::{ImplementsKw, LetIntroducer, MemberSep, NameToken, StringToken},
    },
    tree::{Bogus, Id, List, Missing},
};

#[expect(deprecated, reason = "the stubs stand in for layers not parsed yet")]
use crate::stubs;
use crate::{
    cursor::{Cursor, Mark},
    error::ParseError,
    parser::{Parse, Parser},
    types::{MEMBER_NAME, any_type, is_at_type_start, reject_unmatched_greater},
};

const HASH_STRING_REMOVED: &str = "Hash string literals like `#\"...\"#` are no longer supported. Use quoted strings or backtick strings instead.";
const TYPE_BUILDER_REMOVED: &str =
    "`type_builder` blocks were removed; runtime type construction is `reflect` (BEP-066)";
const HEADER_COMMENT_MISPLACED: &str =
    "a `//#` header is a statement: it is only allowed inside a block";

/// A keyword that always starts a fresh item at module scope; inside a body
/// it means a closing brace is missing.
const TOP_LEVEL_KEYWORD: KindSet = KindSet::new(&[
    TokenKind::Class,
    TokenKind::Enum,
    TokenKind::Interface,
    TokenKind::Function,
    TokenKind::Implements,
    TokenKind::Implement,
    TokenKind::Client,
    TokenKind::Generator,
    TokenKind::Test,
    TokenKind::Testset,
    TokenKind::RetryPolicy,
    TokenKind::TemplateString,
]);

/// Reserved words of the interface syntax remain valid method and field-link
/// names — notably on the reflection types.
const FUNCTION_NAME: KindSet = Ident::KINDS.union(KindSet::new(&[
    TokenKind::Implements,
    TokenKind::Implement,
    TokenKind::Extends,
    TokenKind::Requires,
    TokenKind::Interface,
]));
const _: () = assert!(FUNCTION_NAME.is_subset(NameToken::KINDS));

/// `@@name` segments: a name, or `throws` (`@@throws` is an attribute).
const BLOCK_ATTR_NAME: KindSet = Ident::KINDS.insert(TokenKind::Throws);
const _: () = assert!(BLOCK_ATTR_NAME.is_subset(NameToken::KINDS));

/// A parameter name: a word, a contextual keyword, or `client`.
pub(crate) const PARAM_NAME: KindSet = Ident::KINDS.insert(TokenKind::Client);
const _: () = assert!(PARAM_NAME.is_subset(NameToken::KINDS));

/// Whether the cursor is at the head of an item no body can nest — `class
/// Name {`, `function name(`, `client<` … — so a body still open here was
/// never closed. Reserved words are also member names (`class Class` is a
/// field), hence the whole head shape. `implements` blocks live in class
/// bodies, `retry_policy` lines in client configs and tests nest in testsets;
/// none of those is a head.
pub(crate) fn at_item_head(cur: Cursor<'_>) -> bool {
    const TYPE_OPENER: KindSet = KindSet::new(&[TokenKind::LBrace, TokenKind::Less]);
    const FUNCTION_OPENER: KindSet = KindSet::new(&[TokenKind::LParen, TokenKind::Less]);
    const CONFIG_OPENER: KindSet = KindSet::new(&[TokenKind::LBrace, TokenKind::LParen]);
    let (name, opener) = match cur.kind(0) {
        Some(TokenKind::Class | TokenKind::Enum | TokenKind::Interface) => {
            (Ident::KINDS, TYPE_OPENER)
        }
        Some(TokenKind::Function) => (FUNCTION_NAME, FUNCTION_OPENER),
        Some(TokenKind::Generator | TokenKind::TemplateString) => (Ident::KINDS, CONFIG_OPENER),
        Some(TokenKind::Client) => return cur.nth_is(1, TokenKind::Less),
        _ => return false,
    };
    cur.kind(1).is_some_and(|kind| name.contains(kind))
        && cur.kind(2).is_some_and(|kind| opener.contains(kind))
}

/// Whether an item can begin at the cursor: a token `item` dispatches on.
/// Where a stray run of text ends.
fn at_item_start(cur: Cursor<'_>) -> bool {
    const ITEM_START: KindSet = TOP_LEVEL_KEYWORD.union(KindSet::new(&[
        TokenKind::Type,
        TokenKind::TypeBuilder,
        TokenKind::Dynamic,
        TokenKind::AtAt,
        TokenKind::DocComment,
        TokenKind::DirectiveComment,
    ]));
    cur.at_set(ITEM_START) || cur.at_in::<LetIntroducer>()
}

/// Where a function signature could resume after a missing name.
const SIGNATURE_RESUME: KindSet = KindSet::new(&[
    TokenKind::LParen,
    TokenKind::Less,
    TokenKind::LBrace,
    TokenKind::Arrow,
    TokenKind::FatArrow,
]);

/// Where a misspelled `type Name = …` ends: a brace or the next declaration
/// keyword the rowan parser stopped at.
const INVALID_ALIAS_END: KindSet = KindSet::new(&[
    TokenKind::LBrace,
    TokenKind::RBrace,
    TokenKind::Class,
    TokenKind::Enum,
    TokenKind::Function,
    TokenKind::Client,
    TokenKind::Generator,
    TokenKind::Test,
]);

// ----- names and strings ------------------------------------------------------------

/// A `Name` from a word or contextual keyword, or a reported `Missing`.
fn expect_name(p: &mut Parser<'_, '_>, expected: &str) -> Result<Id<Name>, Missing> {
    p.expect_name(Ident::KINDS, expected)
}

pub(crate) fn reject_reserved_trace_parameter(p: &mut Parser<'_, '_>) {
    if p.cur.text_at(0) == Some("$trace") {
        p.error_here(
            "`$trace` is reserved for invocation tracing and cannot be declared as a parameter",
        );
    }
}

/// Where a string token is read: what is reported for it, and how.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StringSite {
    /// An expression: everything is reported, and an unknown escape's message
    /// offers a raw string as the fix.
    Expr,
    /// A type, a pattern or an attribute argument, where a raw string does
    /// not parse: everything is reported, with no raw-string fix.
    Literal,
    /// An LLM prompt: a raw string is a fix, but a removed `#"…"#` here keeps
    /// the prompt-specific migration diagnostic lowering emits instead of
    /// also getting the general one.
    Prompt,
    /// The body of a removed construct: only an unclosed literal is reported;
    /// the removal diagnostic covers the rest.
    Removed,
}

impl StringSite {
    /// Whether a removed `#"…"#` string is reported where it is read.
    fn reports_removed_hash(self) -> bool {
        match self {
            StringSite::Expr | StringSite::Literal => true,
            StringSite::Prompt | StringSite::Removed => false,
        }
    }

    /// Whether escape sequences are checked here, and if so whether an
    /// unknown escape's message offers `r"…"` as the fix.
    fn escape_check(self) -> Option<RawStringFix> {
        match self {
            StringSite::Expr | StringSite::Prompt => Some(RawStringFix::Offered),
            StringSite::Literal => Some(RawStringFix::Unavailable),
            StringSite::Removed => None,
        }
    }
}

/// Whether an unknown escape's message offers a raw string: only where one
/// would parse.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RawStringFix {
    Offered,
    Unavailable,
}

/// Report what the string token `idx` carries: an unclosed literal, the
/// removed `#"…"#` form, or escape sequences that do not decode.
pub(crate) fn string_token_diagnostics(p: &mut Parser<'_, '_>, idx: TokenIdx, site: StringSite) {
    let report_removed = site.reports_removed_hash();
    let range = p.cur.token_range(idx);
    match p.cur.tokens().kind(idx) {
        TokenKind::UnclosedString => p.unexpected("Unclosed string literal"),
        TokenKind::UnclosedRawString => p.unexpected("Unclosed raw string literal"),
        TokenKind::UnclosedByteString => p.unexpected("Unclosed byte string literal"),
        TokenKind::UnclosedHashString => {
            let hashes = p
                .cur
                .text_of(idx)
                .bytes()
                .take_while(|&b| b == b'#')
                .count();
            p.unexpected(format!(
                "Unclosed raw string (expected \"{}\")",
                "#".repeat(hashes)
            ));
            if report_removed {
                p.removed(HASH_STRING_REMOVED, range);
            }
        }
        TokenKind::LegacyHashString => {
            if report_removed {
                p.removed(HASH_STRING_REMOVED, range);
            }
        }
        TokenKind::String | TokenKind::ByteString => {
            if let Some(fix) = site.escape_check() {
                report_escape_issues(p, idx, fix);
            }
        }
        TokenKind::RawString => {}
        other => unreachable!("not a string token: {other:?}"),
    }
}

/// Report every escape sequence in the body of the closed `"…"` or `b"…"`
/// literal at `idx` that does not decode — once, here, however many readers
/// decode the literal later (they keep such sequences verbatim). An unclosed
/// literal is not checked: its missing closer is its diagnostic.
fn report_escape_issues(p: &mut Parser<'_, '_>, idx: TokenIdx, fix: RawStringFix) {
    use baml_base::escape::{byte_string_literal_escape_issues, string_literal_escape_issues};
    let text = p.cur.text_of(idx);
    debug_assert!(
        text.len() >= 2 && text.ends_with('"'),
        "a closed literal: {text:?}"
    );
    let (opener, issues) = match p.cur.tokens().kind(idx) {
        TokenKind::String => (1, string_literal_escape_issues(&text[1..text.len() - 1])),
        TokenKind::ByteString => (
            2,
            byte_string_literal_escape_issues(&text[2..text.len() - 1]),
        ),
        other => unreachable!("not an escaped string token: {other:?}"),
    };
    let body_start = p.cur.token_range(idx).start() + text_size::TextSize::from(opener);
    report_escape_issues_from(p, body_start, issues, fix);
}

/// Report `issues` found in literal text that begins at `body_start`.
pub(crate) fn report_escape_issues_from(
    p: &mut Parser<'_, '_>,
    body_start: text_size::TextSize,
    issues: Vec<baml_base::escape::EscapeIssue>,
    fix: RawStringFix,
) {
    let offset = |n: usize| {
        text_size::TextSize::try_from(n)
            .unwrap_or_else(|_| unreachable!("an offset inside a lexed token fits in u32"))
    };
    for mut issue in issues {
        if fix == RawStringFix::Unavailable {
            issue.suggest_raw_string = false;
        }
        let start = body_start + offset(issue.start);
        let range = text_size::TextRange::new(start, start + offset(issue.len));
        p.out
            .report(ParseError::InvalidStringEscape { issue, range });
    }
}

/// A string literal at the current token, with its diagnostics.
fn string_lit(p: &mut Parser<'_, '_>, site: StringSite) -> Option<Id<StringLit>> {
    let token = p.cur.eat_in::<StringToken>()?;
    string_token_diagnostics(p, token.idx, site);
    Some(p.alloc(Mark::of(token), StringLit { token }))
}

/// A string literal or a backtick template.
#[expect(deprecated, reason = "a stub stands in for a layer not parsed yet")]
fn any_string_like(p: &mut Parser<'_, '_>, site: StringSite) -> Option<AnyStringLike> {
    if let Some(lit) = string_lit(p, site) {
        return Some(AnyStringLike::String(lit));
    }
    p.cur
        .at(TokenKind::BacktickStart)
        .then(|| AnyStringLike::Bogus(stubs::stub_backtick(p)))
}

// ----- attributes --------------------------------------------------------------------

/// `name.segment(args)` after the `@` of a field or variant attribute.
impl Parse for Attribute {
    type Head = Token!["@"];
    fn parse(p: &mut Parser<'_, '_>, at: Self::Head) -> Id<Self> {
        let mark = Mark::of(at);
        if p.cur.has_newline_ahead() {
            // The name must share the `@`'s line.
            let range = p.cur.token_range(at.idx);
            let path = Err(p.missing_because("attribute is missing a name", range));
            return p.alloc(
                mark,
                Attribute {
                    at,
                    path,
                    args: None,
                },
            );
        }
        let path = match p.cur.eat_set::<NameToken>(Ident::KINDS) {
            Some(head) => Ok(dotted_name(p, head, Ident::KINDS)),
            None => Err(p.missing("attribute name")),
        };
        let args = attr_args(p, path.is_ok());
        p.alloc(mark, Attribute { at, path, args })
    }
}

/// The `(args)` of an attribute whose name is there.
fn attr_args(p: &mut Parser<'_, '_>, named: bool) -> Option<Id<AttrArgs>> {
    if !named {
        return None;
    }
    p.cur.eat::<punct::LParen>().map(|l| AttrArgs::parse(p, l))
}

/// The `@attr`s trailing a field or enum variant. Attributes attach to
/// declarations, never to types: a type parse stops at `@` and leaves them to
/// the declaration that owns them.
fn trailing_attributes(p: &mut Parser<'_, '_>) -> List<Id<Attribute>> {
    let mut attrs = Vec::new();
    while let Some(at) = p.cur.eat::<punct::At>() {
        attrs.push(Attribute::parse(p, at));
    }
    p.out.alloc_list(attrs)
}

/// `name.segment(args)` after the `@@` of a declaration attribute.
impl Parse for BlockAttr {
    type Head = Token!["@@"];
    fn parse(p: &mut Parser<'_, '_>, at: Self::Head) -> Id<Self> {
        let path = match p.cur.eat_set::<NameToken>(BLOCK_ATTR_NAME) {
            Some(head) => Ok(dotted_name(p, head, BLOCK_ATTR_NAME)),
            None => Err(p.missing("attribute name")),
        };
        let args = attr_args(p, path.is_ok());
        p.alloc(Mark::of(at), BlockAttr { at, path, args })
    }
}

/// The `@@attr`s before an item or member, parsed before anything decides
/// what they belong to: the caller dispatches on the token after them.
fn leading_block_attrs(p: &mut Parser<'_, '_>) -> Vec<Id<BlockAttr>> {
    let mut attrs = Vec::new();
    while let Some(at) = p.cur.eat::<punct::AtAt>() {
        attrs.push(BlockAttr::parse(p, at));
    }
    attrs
}

/// `b.c` after the first name `head` of `a.b.c`, each later segment one of
/// `segment`.
fn dotted_name(
    p: &mut Parser<'_, '_>,
    head: Token![NameToken],
    segment: KindSet,
) -> Id<DottedName> {
    let mark = Mark::of(head);
    let head = p.name(head);
    let segments = path_segments(p, |p| {
        p.expect_name(segment, "attribute name segment after dot")
    });
    let segments = p.out.alloc_list(segments);
    p.alloc(mark, DottedName { head, segments })
}

/// `.name` segments after a path head, for as long as a `.` follows. A dot
/// with no name after it (`segment` says so with a `Missing`) ends the path.
pub(crate) fn path_segments(
    p: &mut Parser<'_, '_>,
    mut segment: impl FnMut(&mut Parser<'_, '_>) -> Result<Id<Name>, Missing>,
) -> Vec<PathSegment> {
    let mut segments = Vec::new();
    while let Some(dot) = p.cur.eat::<punct::Dot>() {
        let name = segment(p);
        let ended = name.is_err();
        segments.push(PathSegment { dot, name });
        if ended {
            break;
        }
    }
    segments
}

impl Parse for AttrArgs {
    type Head = Token!["("];
    fn parse(p: &mut Parser<'_, '_>, l: Self::Head) -> Id<Self> {
        let args = p.sep_list(TokenKind::RParen, |p| attribute_arg(p));
        let r = p.expect::<punct::RParen>();
        p.alloc(Mark::of(l), AttrArgs { l, args, r })
    }
}

/// `"text"`, a backtick template, `{{ jinja }}`, or one bare word.
#[expect(deprecated, reason = "a stub stands in for a layer not parsed yet")]
fn attribute_arg(p: &mut Parser<'_, '_>) -> Option<AnyAttrArg> {
    if let Some(lit) = string_lit(p, StringSite::Literal) {
        return Some(AnyAttrArg::String(lit));
    }
    if p.cur.at(TokenKind::BacktickStart) {
        return Some(AnyAttrArg::Bogus(stubs::stub_backtick(p)));
    }
    if p.cur.at(TokenKind::LBrace)
        && p.cur.nth_is(1, TokenKind::LBrace)
        && let Some(l1) = p.cur.eat::<punct::LBrace>()
        && let Some(l2) = p.cur.eat::<punct::LBrace>()
    {
        return Some(AnyAttrArg::Jinja(JinjaArg::parse(p, (l1, l2))));
    }
    if let Some(name) = p.eat_name(Ident::KINDS) {
        return Some(AnyAttrArg::Unquoted(name));
    }
    p.unexpected("attribute argument");
    None
}

/// `… }}` after the `{{`, kept as its tokens.
impl Parse for JinjaArg {
    type Head = (Token!["{"], Token!["{"]);
    fn parse(p: &mut Parser<'_, '_>, (l1, l2): Self::Head) -> Id<Self> {
        let mark = Mark::of(l1);
        let body = p.cur.mark();
        while !(p.cur.at_end() || p.cur.at(TokenKind::RBrace) && p.cur.nth_is(1, TokenKind::RBrace))
        {
            p.cur.skip();
        }
        let tokens = p.cur.range_from(body);
        let r1 = p.expect::<punct::RBrace>();
        let r2 = p.expect::<punct::RBrace>();
        p.alloc(
            mark,
            JinjaArg {
                l1,
                l2,
                tokens,
                r1,
                r2,
            },
        )
    }
}

// ----- generics and parameters ---------------------------------------------------------

/// Declaration-site `T, U extends A & B>` after its `<`.
impl Parse for GenericParamList {
    type Head = Token!["<"];
    fn parse(p: &mut Parser<'_, '_>, l: Self::Head) -> Id<Self> {
        let params = p.sep_list(TokenKind::Greater, |p| generic_param(p));
        let r = p.expect::<punct::Greater>();
        p.alloc(Mark::of(l), GenericParamList { l, params, r })
    }
}

fn generic_param(p: &mut Parser<'_, '_>) -> Option<Id<GenericParam>> {
    let mark = p.cur.mark();
    let Some(name) = p.eat_name(Ident::KINDS) else {
        p.unexpected("type parameter name");
        return None;
    };
    let bounds = p
        .cur
        .eat::<kw::Extends>()
        .map(|kw| GenericParamBounds::parse(p, kw));
    Some(p.alloc(mark, GenericParam { name, bounds }))
}

/// `A & B` after the `extends` of a generic parameter or a `let type` binding.
impl Parse for GenericParamBounds {
    type Head = Token!["extends"];
    fn parse(p: &mut Parser<'_, '_>, kw: Self::Head) -> Id<Self> {
        let mark = Mark::of(kw);
        let first = any_type(p);
        let mut rest = Vec::new();
        while let Some(and) = p.cur.eat::<punct::And>() {
            let ty = any_type(p);
            let ended = ty.is_err();
            rest.push(BoundArm { and, ty });
            if ended {
                break;
            }
        }
        let rest = p.out.alloc_list(rest);
        if p.cur.at(TokenKind::As) {
            // Consumed for recovery and rejected; the tokens stay unowned.
            p.error_here("generic bound aliases are not supported; use `.as<Interface<...>>()` at call sites");
            p.cur.skip();
            if p.cur.at_ident() {
                p.cur.skip();
            } else {
                p.unexpected("alias name");
            }
        }
        p.alloc(mark, GenericParamBounds { kw, first, rest })
    }
}

/// `a: A, b: B = default)` after its `(`.
impl Parse for ParamList {
    type Head = Token!["("];
    fn parse(p: &mut Parser<'_, '_>, l: Self::Head) -> Id<Self> {
        let params = p.sep_list(TokenKind::RParen, |p| param(p));
        let r = p.expect::<punct::RParen>();
        p.alloc(Mark::of(l), ParamList { l, params, r })
    }
}

/// `name: T = default`; `self` takes no type. The colon is optional, but
/// without it the type must share the name's line.
#[expect(deprecated, reason = "a stub stands in for a layer not parsed yet")]
fn param(p: &mut Parser<'_, '_>) -> Option<Id<Param>> {
    let mark = p.cur.mark();
    reject_reserved_trace_parameter(p);
    let is_self = p.cur.at(TokenKind::SelfValue);
    let name = if let Some(name) = p.eat_name(PARAM_NAME) {
        Ok(name)
    } else if is_at_type_start(p.cur) {
        // A bare type: the name is missing, the type is kept.
        Err(p.missing("parameter name"))
    } else {
        p.unexpected("parameter name");
        return None;
    };
    let ascription = if is_self {
        None
    } else {
        let ascription_mark = p.cur.mark();
        let colon = p.cur.eat::<punct::Colon>();
        let newline_before_type = p.cur.has_newline_ahead();
        let ty = if is_at_type_start(p.cur) && (!newline_before_type || colon.is_some()) {
            any_type(p)
        } else {
            Err(p.missing("type annotation"))
        };
        reject_unmatched_greater(p);
        Some(p.alloc(ascription_mark, TypeAscription { colon, ty }))
    };
    // `self` consumes a default too, so lowering can reject it specifically.
    let default = p.cur.eat::<punct::Equals>().map(|eq| {
        let value = stubs::stub_expr(p, &[]);
        p.alloc(Mark::of(eq), DefaultValue { eq, value })
    });
    Some(p.alloc(
        mark,
        Param {
            name,
            ascription,
            default,
        },
    ))
}

// ----- functions ------------------------------------------------------------------------

/// The head every function-shaped member shares:
/// `function name<G>(params) -> R throws E`, after its `@@attrs`.
struct Signature {
    attrs: List<Id<BlockAttr>>,
    kw: Token!["function"],
    name: Result<Id<Name>, Missing>,
    generics: Option<Id<GenericParamList>>,
    params: Result<Id<ParamList>, Missing>,
    ret: Result<Id<ReturnType>, Missing>,
    throws: Option<Id<ThrowsClause>>,
    /// Whether a `{` body may be an LLM body: not after `-> {` (the brace
    /// is the body, and `client` text inside must not masquerade as an LLM
    /// field) nor after a missing return type.
    allow_llm_body: bool,
}

/// After `function`, with the `@@attrs` already parsed.
fn signature(
    p: &mut Parser<'_, '_>,
    attrs: Vec<Id<BlockAttr>>,
    kw: Token!["function"],
) -> Signature {
    let attrs = p.out.alloc_list(attrs);

    let name = match p.eat_name(FUNCTION_NAME) {
        Some(name) => Ok(name),
        None => {
            let missing = p.missing("function name");
            // Recovery: skip to where the signature could resume.
            p.cur.skip_until(SIGNATURE_RESUME);
            Err(missing)
        }
    };

    let generics = p
        .cur
        .eat::<punct::Less>()
        .map(|l| GenericParamList::parse(p, l));

    // Old-style `function Name {`: one diagnostic for the whole missing head.
    if p.cur.at(TokenKind::LBrace) {
        let range = p.cur.error_range();
        let missing = p.missing_because(
            "old-style function syntax; use `function Name(params...) -> ReturnType { ... }`",
            range,
        );
        return Signature {
            attrs,
            kw,
            name,
            generics,
            params: Err(missing),
            ret: Err(missing),
            throws: None,
            allow_llm_body: false,
        };
    }

    let params = match p.cur.eat::<punct::LParen>() {
        Some(l) => Ok(ParamList::parse(p, l)),
        None => Err(p.missing("'('")),
    };

    // `=>` is accepted in place of `->`; the formatter canonicalizes.
    let mut allow_llm_body = true;
    let ret = if let Some(arrow) = p.cur.eat_in::<baml_syntax::token::sets::AnyArrow>() {
        if p.cur.at(TokenKind::LBrace) {
            allow_llm_body = false;
        }
        let ty = any_type(p);
        reject_unmatched_greater(p);
        Ok(p.alloc(Mark::of(arrow), ReturnType { arrow, ty }))
    } else {
        allow_llm_body = false;
        Err(p.missing("return type (->)"))
    };

    let throws = p
        .cur
        .eat::<kw::Throws>()
        .map(|kw| ThrowsClause::parse(p, kw));

    Signature {
        attrs,
        kw,
        name,
        generics,
        params,
        ret,
        throws,
        allow_llm_body,
    }
}

/// `… { body }` after `@@attrs function` — a function, method or default
/// method, whose body is required. `mark` is where the attributes began.
fn function_def(
    p: &mut Parser<'_, '_>,
    mark: Mark,
    attrs: Vec<Id<BlockAttr>>,
    kw: Token!["function"],
) -> Id<FunctionDef> {
    let sig = signature(p, attrs, kw);
    let body = match p.cur.eat::<punct::LBrace>() {
        Some(l) => Ok(function_body(p, l, sig.allow_llm_body)),
        None => Err(p.missing("function body")),
    };
    p.alloc(
        mark,
        FunctionDef {
            attrs: sig.attrs,
            kw: sig.kw,
            name: sig.name,
            generics: sig.generics,
            params: sig.params,
            ret: sig.ret,
            throws: sig.throws,
            body,
        },
    )
}

/// A method in an interface: a definition when a body follows the signature,
/// otherwise a required signature (`;` optional). Decided by the token after
/// the signature, not by a scan ahead of it.
fn interface_method(
    p: &mut Parser<'_, '_>,
    mark: Mark,
    attrs: Vec<Id<BlockAttr>>,
    kw: Token!["function"],
) -> AnyInterfaceMember {
    let sig = signature(p, attrs, kw);
    if let Some(l) = p.cur.eat::<punct::LBrace>() {
        let body = Ok(function_body(p, l, sig.allow_llm_body));
        return AnyInterfaceMember::Method(p.alloc(
            mark,
            FunctionDef {
                attrs: sig.attrs,
                kw: sig.kw,
                name: sig.name,
                generics: sig.generics,
                params: sig.params,
                ret: sig.ret,
                throws: sig.throws,
                body,
            },
        ));
    }
    let semi = p.cur.eat::<punct::Semicolon>();
    AnyInterfaceMember::MethodSig(p.alloc(
        mark,
        MethodSig {
            attrs: sig.attrs,
            kw: sig.kw,
            name: sig.name,
            generics: sig.generics,
            params: sig.params,
            ret: sig.ret,
            throws: sig.throws,
            semi,
        },
    ))
}

/// After the body's `{`: an LLM body when allowed and its first member says
/// so, otherwise an expression body.
#[expect(deprecated, reason = "a stub stands in for a layer not parsed yet")]
fn function_body(p: &mut Parser<'_, '_>, l: Token!["{"], allow_llm_body: bool) -> AnyFunctionBody {
    if allow_llm_body
        && p.cur
            .looks_like_llm_function_body_from(p.cur.sig_index(l.idx))
    {
        AnyFunctionBody::Llm(LlmBody::parse(p, l))
    } else {
        AnyFunctionBody::Bogus(stubs::stub_block(p, l))
    }
}

impl Parse for LlmBody {
    type Head = Token!["{"];
    fn parse(p: &mut Parser<'_, '_>, l: Self::Head) -> Id<Self> {
        let mark = Mark::of(l);
        let mut fields = Vec::new();
        let (mut has_client, mut has_prompt, mut has_tools) = (false, false, false);
        while !p.cur.at(TokenKind::RBrace) && !p.cur.at_end() {
            // A top-level keyword other than `client` means a `}` is missing.
            if p.cur.at_set(TOP_LEVEL_KEYWORD) && !p.cur.at(TokenKind::Client) {
                break;
            }
            if p.cur.at(TokenKind::Client) && has_client {
                p.unexpected("Duplicate 'client' field");
            } else if p.cur.at(TokenKind::Prompt) && has_prompt {
                p.unexpected("Duplicate 'prompt' field");
            } else if p.cur.at(TokenKind::Tools) && has_tools {
                p.unexpected("Duplicate 'tools' field");
            }
            if let Some(kw) = p.cur.eat::<kw::Client>() {
                has_client = true;
                fields.push(AnyLlmField::Client(ClientField::parse(p, kw)));
            } else if let Some(kw) = p.cur.eat::<ctx::Prompt>() {
                has_prompt = true;
                fields.push(AnyLlmField::Prompt(PromptField::parse(p, kw)));
            } else if let Some(kw) = p.cur.eat::<ctx::Tools>() {
                has_tools = true;
                fields.push(AnyLlmField::Tools(ToolsField::parse(p, kw)));
            } else if at_removed_type_builder(p.cur)
                && let Some(kw) = p.cur.eat::<ctx::TypeBuilder>()
            {
                fields.push(AnyLlmField::Bogus(skip_removed_type_builder(p, kw)));
            } else if p.cur.at(TokenKind::Comma) || p.cur.at(TokenKind::Semicolon) {
                // Fields are separated by a newline; name the real requirement
                // instead of letting the value scanner swallow the separator
                // and misreport `prompt` as missing.
                let sep = p.cur.text_at(0).unwrap_or_default();
                let error = ParseError::InvalidSyntax {
                    message: format!(
                        "unexpected `{sep}` between LLM function fields; separate `client` and `prompt` with a newline"
                    ),
                    range: p.cur.error_range(),
                };
                let sep_mark = p.cur.mark();
                p.cur.skip();
                fields.push(AnyLlmField::Bogus(p.bogus_since(sep_mark, error)));
            } else {
                // A name is quoted as written; anything else by its kind, so
                // the message never embeds a whole string literal.
                let found = match p.cur.kind(0) {
                    Some(kind) if Ident::KINDS.contains(kind) => {
                        p.cur.text_at(0).unwrap_or_default().to_string()
                    }
                    Some(kind) => crate::parser::found_name(kind),
                    None => "EOF".to_string(),
                };
                let expected = format!(
                    "Only 'client', 'tools' and 'prompt' allowed in LLM function, found '{found}'"
                );
                fields.push(AnyLlmField::Bogus(p.bogus_one(expected)));
            }
        }
        if !has_client {
            p.unexpected("LLM function missing 'client' field");
        }
        if !has_prompt {
            p.unexpected("LLM function missing 'prompt' field");
        }
        let fields = p.out.alloc_list(fields);
        let r = p.expect::<punct::RBrace>();
        p.alloc(mark, LlmBody { l, fields, r })
    }
}

/// `client: "provider/model"` or `client: <expr>`. The legacy unquoted
/// `openai/gpt-4o` is an expression here (a division) that lowering rejects
/// with a migration error.
impl Parse for ClientField {
    type Head = Token!["client"];
    #[expect(deprecated, reason = "a stub stands in for a layer not parsed yet")]
    fn parse(p: &mut Parser<'_, '_>, kw: Self::Head) -> Id<Self> {
        let mark = Mark::of(kw);
        let colon = p.expect::<punct::Colon>();
        let value = if p.cur.at(TokenKind::String) || p.cur.at(TokenKind::UnclosedString) {
            let lit = string_lit(p, StringSite::Expr)
                .unwrap_or_else(|| unreachable!("checked a string token"));
            Ok(baml_syntax::nodes::AnyExpr::String(lit))
        } else if p.cur.at(TokenKind::RBrace) || p.cur.has_newline_ahead() || p.cur.at_end() {
            Err(p.missing("client value"))
        } else {
            stubs::stub_expr(p, &[])
        };
        p.alloc(mark, ClientField { kw, colon, value })
    }
}

impl Parse for ToolsField {
    type Head = Token!["tools"];
    #[expect(deprecated, reason = "a stub stands in for a layer not parsed yet")]
    fn parse(p: &mut Parser<'_, '_>, kw: Self::Head) -> Id<Self> {
        let mark = Mark::of(kw);
        let colon = p.expect::<punct::Colon>();
        let value = stubs::stub_expr(p, &[]);
        p.alloc(mark, ToolsField { kw, colon, value })
    }
}

/// `prompt: "…"` or a backtick template. A removed hash string here keeps the
/// prompt-specific migration diagnostic lowering emits instead of also
/// reporting the general one.
impl Parse for PromptField {
    type Head = Token!["prompt"];
    fn parse(p: &mut Parser<'_, '_>, kw: Self::Head) -> Id<Self> {
        let mark = Mark::of(kw);
        let colon = p.expect::<punct::Colon>();
        let value =
            any_string_like(p, StringSite::Prompt).ok_or_else(|| p.missing("prompt string"));
        p.alloc(mark, PromptField { kw, colon, value })
    }
}

// ----- enums ------------------------------------------------------------------------------

/// After `enum`, with the `@@attrs` already parsed; `mark` is where they began.
fn enum_def(
    p: &mut Parser<'_, '_>,
    mark: Mark,
    attrs: Vec<Id<BlockAttr>>,
    kw: Token!["enum"],
) -> Id<EnumDef> {
    {
        let attrs = p.out.alloc_list(attrs);
        let name = expect_name(p, "enum name");
        let l = match p.expect::<punct::LBrace>() {
            Ok(l) => l,
            Err(missing) => {
                let members = p.out.alloc_list(Vec::new());
                return p.alloc(
                    mark,
                    EnumDef {
                        attrs,
                        kw,
                        name,
                        l: Err(missing),
                        members,
                        r: Err(missing),
                    },
                );
            }
        };
        let mut members = Vec::new();
        while !p.cur.at(TokenKind::RBrace) && !p.cur.at_end() {
            // A top-level keyword means the closing brace is missing.
            if p.cur.at_set(TOP_LEVEL_KEYWORD) {
                break;
            }
            if let Some(at) = p.cur.eat::<punct::AtAt>() {
                members.push(AnyEnumMember::BlockAttr(BlockAttr::parse(p, at)));
            } else if let Some(name) = p.cur.eat_set::<NameToken>(Ident::KINDS) {
                members.push(AnyEnumMember::Variant(EnumVariant::parse(p, name)));
            } else {
                members.push(AnyEnumMember::Bogus(
                    p.bogus_one("Unexpected token in enum body"),
                ));
            }
        }
        let members = p.out.alloc_list(members);
        let r = p.expect::<punct::RBrace>();
        p.alloc(
            mark,
            EnumDef {
                attrs,
                kw,
                name,
                l: Ok(l),
                members,
                r,
            },
        )
    }
}

/// A variant after its name.
impl Parse for EnumVariant {
    type Head = Token![NameToken];
    fn parse(p: &mut Parser<'_, '_>, name: Self::Head) -> Id<Self> {
        let mark = Mark::of(name);
        let name = p.name(name);
        // `A int | string` is a type annotation (an error), `A\n    B` is the
        // next variant: a following name is a type only when a modifier
        // follows it; literals and `(` can only be types.
        let has_type_annotation = if p.cur.at_ident() {
            matches!(
                p.cur.kind(1),
                Some(TokenKind::Pipe | TokenKind::LBracket | TokenKind::Question | TokenKind::Less)
            )
        } else {
            p.cur.at_in::<StringToken>()
                || matches!(
                    p.cur.kind(0),
                    Some(
                        TokenKind::Bigint
                            | TokenKind::Integer
                            | TokenKind::Float
                            | TokenKind::LParen
                    )
                )
        };
        if has_type_annotation {
            // Parsed for its extent and its own diagnostics, then discarded.
            let ty_mark = p.cur.mark();
            p.discard(|p| {
                let _ = any_type(p);
            });
            let range = p.cur.text_range(p.cur.range_from(ty_mark));
            p.error("enum variants cannot have type annotations", range);
        }
        let attrs = trailing_attributes(p);
        // Commas are canonical; a semicolon is a slip the formatter repairs.
        let sep = p.cur.eat_in::<MemberSep>();
        p.alloc(mark, EnumVariant { name, attrs, sep })
    }
}

// ----- classes -----------------------------------------------------------------------------

/// After `class`, with the `@@attrs` already parsed; `mark` is where they began.
fn class_def(
    p: &mut Parser<'_, '_>,
    mark: Mark,
    attrs: Vec<Id<BlockAttr>>,
    kw: Token!["class"],
) -> Id<ClassDef> {
    {
        let attrs = p.out.alloc_list(attrs);
        let name = expect_name(p, "class name");
        let generics = p
            .cur
            .eat::<punct::Less>()
            .map(|l| GenericParamList::parse(p, l));
        let l = match p.expect::<punct::LBrace>() {
            Ok(l) => l,
            Err(missing) => {
                let members = p.out.alloc_list(Vec::new());
                return p.alloc(
                    mark,
                    ClassDef {
                        attrs,
                        kw,
                        name,
                        generics,
                        l: Err(missing),
                        members,
                        r: Err(missing),
                    },
                );
            }
        };
        let mut members = Vec::new();
        while !p.cur.at(TokenKind::RBrace) && !p.cur.at_end() {
            // A top-level keyword (other than the members a class may hold)
            // means the closing brace is missing. `interface` and `client` are
            // also member names, so their declaration shapes decide.
            let recover_top_level_item = if p.cur.at(TokenKind::Interface) {
                looks_like_interface_declaration_start(p.cur)
            } else if p.cur.at(TokenKind::Client) {
                looks_like_client_declaration_start(p.cur)
            } else {
                p.cur.at_set(TOP_LEVEL_KEYWORD)
            };
            if recover_top_level_item
                && !p.cur.at(TokenKind::Function)
                && !p.cur.at(TokenKind::Implements)
                && !p.cur.at(TokenKind::Implement)
            {
                break;
            }
            // `@@attrs` belong to a method that follows them; otherwise each
            // is a class-level attribute in its own right.
            let member_mark = p.cur.mark();
            let attrs = leading_block_attrs(p);
            if let Some(kw) = p.cur.eat::<kw::Function>() {
                members.push(AnyClassMember::Method(function_def(
                    p,
                    member_mark,
                    attrs,
                    kw,
                )));
            } else if !attrs.is_empty() {
                members.extend(attrs.into_iter().map(AnyClassMember::BlockAttr));
            } else if let Some(kw) = p.cur.eat_in::<ImplementsKw>() {
                members.push(AnyClassMember::Implements(ImplementsBlock::parse(p, kw)));
            } else if let Some(name) = p.cur.eat_set::<NameToken>(MEMBER_NAME) {
                members.push(AnyClassMember::Field(Field::parse(p, name)));
            } else {
                members.push(AnyClassMember::Bogus(
                    p.bogus_one("Unexpected token in class body"),
                ));
            }
        }
        let members = p.out.alloc_list(members);
        let r = p.expect::<punct::RBrace>();
        p.alloc(
            mark,
            ClassDef {
                attrs,
                kw,
                name,
                generics,
                l: Ok(l),
                members,
                r,
            },
        )
    }
}

/// `name: T @attr,` — the colon is optional, but without it the type must
/// share the name's line.
/// A field after its name.
impl Parse for Field {
    type Head = Token![NameToken];
    fn parse(p: &mut Parser<'_, '_>, name: Self::Head) -> Id<Self> {
        let mark = Mark::of(name);
        let name_idx = name.idx;
        let name = p.name(name);
        let colon = p.cur.eat::<punct::Colon>();
        let newline_before_type = p.cur.has_newline_ahead();
        let ty = if is_at_type_start(p.cur) && (!newline_before_type || colon.is_some()) {
            any_type(p)
        } else {
            let text = p.cur.text_of(name_idx);
            let message = format!("field '{text}' is missing a type annotation");
            let range = p.cur.token_range(name_idx);
            Err(p.missing_because(message, range))
        };
        reject_unmatched_greater(p);
        let attrs = trailing_attributes(p);
        let sep = p.cur.eat_in::<MemberSep>();
        p.alloc(
            mark,
            Field {
                name,
                colon,
                ty,
                attrs,
                sep,
            },
        )
    }
}

// ----- interfaces -----------------------------------------------------------------------------

/// After `interface`, with the `@@attrs` already parsed; `mark` is where they
/// began.
fn interface_def(
    p: &mut Parser<'_, '_>,
    mark: Mark,
    attrs: Vec<Id<BlockAttr>>,
    kw: Token!["interface"],
) -> Id<InterfaceDef> {
    {
        let attrs = p.out.alloc_list(attrs);
        let name = expect_name(p, "interface name");
        let generics = p
            .cur
            .eat::<punct::Less>()
            .map(|l| GenericParamList::parse(p, l));
        // Interfaces do not extend each other: `requires` says implementors
        // must also implement the listed interfaces.
        let requires = p
            .cur
            .eat::<kw::Requires>()
            .map(|kw| RequiresClause::parse(p, kw));
        let l = match p.expect::<punct::LBrace>() {
            Ok(l) => l,
            Err(missing) => {
                let members = p.out.alloc_list(Vec::new());
                return p.alloc(
                    mark,
                    InterfaceDef {
                        attrs,
                        kw,
                        name,
                        generics,
                        requires,
                        l: Err(missing),
                        members,
                        r: Err(missing),
                    },
                );
            }
        };
        let mut members = Vec::new();
        while !p.cur.at(TokenKind::RBrace) && !p.cur.at_end() {
            if p.cur.at_set(TOP_LEVEL_KEYWORD) && !p.cur.at(TokenKind::Function) {
                break;
            }
            let member_mark = p.cur.mark();
            let attrs = leading_block_attrs(p);
            if let Some(kw) = p.cur.eat::<kw::Function>() {
                members.push(interface_method(p, member_mark, attrs, kw));
            } else if !attrs.is_empty() {
                members.extend(attrs.into_iter().map(AnyInterfaceMember::BlockAttr));
            } else if let Some(kw) = p.cur.eat::<ctx::Type>() {
                members.push(AnyInterfaceMember::AssocType(AssocTypeDecl::parse(p, kw)));
            } else if let Some(name) = p.cur.eat_set::<NameToken>(Ident::KINDS) {
                members.push(AnyInterfaceMember::Field(Field::parse(p, name)));
            } else {
                members.push(AnyInterfaceMember::Bogus(
                    p.bogus_one("Unexpected token in interface body"),
                ));
            }
        }
        let members = p.out.alloc_list(members);
        let r = p.expect::<punct::RBrace>();
        p.alloc(
            mark,
            InterfaceDef {
                attrs,
                kw,
                name,
                generics,
                requires,
                l: Ok(l),
                members,
                r,
            },
        )
    }
}

/// `I1, I2` after an interface's `requires`.
impl Parse for RequiresClause {
    type Head = Token!["requires"];
    fn parse(p: &mut Parser<'_, '_>, kw: Self::Head) -> Id<Self> {
        let mark = Mark::of(kw);
        let types = if is_at_type_start(p.cur) {
            let first = any_type(p).unwrap_or_else(|_| unreachable!("a type starts here"));
            // A trailing comma before the body is allowed.
            Ok(p.sep1_list(first, |p| {
                if p.cur.at(TokenKind::LBrace) {
                    None
                } else if is_at_type_start(p.cur) {
                    any_type(p).ok()
                } else {
                    p.unexpected("interface name");
                    None
                }
            }))
        } else {
            Err(p.missing("interface name"))
        };
        p.alloc(mark, RequiresClause { kw, types })
    }
}

// ----- implementations -------------------------------------------------------------------------

/// `I { … }` after the `implements` inside a class.
impl Parse for ImplementsBlock {
    type Head = Token![ImplementsKw];
    fn parse(p: &mut Parser<'_, '_>, kw: Self::Head) -> Id<Self> {
        let mark = Mark::of(kw);
        let target = if is_at_type_start(p.cur) {
            any_type(p)
        } else {
            Err(p.missing("interface name"))
        };
        let l = match p.expect::<punct::LBrace>() {
            Ok(l) => l,
            Err(missing) => {
                let members = p.out.alloc_list(Vec::new());
                return p.alloc(
                    mark,
                    ImplementsBlock {
                        kw,
                        target,
                        l: Err(missing),
                        members,
                        r: Err(missing),
                    },
                );
            }
        };
        let members = impl_members(p);
        let r = p.expect::<punct::RBrace>();
        p.alloc(
            mark,
            ImplementsBlock {
                kw,
                target,
                l: Ok(l),
                members,
                r,
            },
        )
    }
}

/// `<G> I for T { … }` after a top-level `implements`.
impl Parse for ImplementsFor {
    type Head = Token![ImplementsKw];
    fn parse(p: &mut Parser<'_, '_>, kw: Self::Head) -> Id<Self> {
        let mark = Mark::of(kw);
        let generics = p
            .cur
            .eat::<punct::Less>()
            .map(|l| GenericParamList::parse(p, l));
        let target = if is_at_type_start(p.cur) {
            any_type(p)
        } else {
            Err(p.missing("interface name"))
        };
        let for_kw = p.expect::<kw::For>();
        let self_ty = if is_at_type_start(p.cur) {
            any_type(p)
        } else {
            Err(p.missing("target type"))
        };
        let l = match p.expect::<punct::LBrace>() {
            Ok(l) => l,
            Err(missing) => {
                let members = p.out.alloc_list(Vec::new());
                return p.alloc(
                    mark,
                    ImplementsFor {
                        kw,
                        generics,
                        target,
                        for_kw,
                        self_ty,
                        l: Err(missing),
                        members,
                        r: Err(missing),
                    },
                );
            }
        };
        let members = impl_members(p);
        let r = p.expect::<punct::RBrace>();
        p.alloc(
            mark,
            ImplementsFor {
                kw,
                generics,
                target,
                for_kw,
                self_ty,
                l: Ok(l),
                members,
                r,
            },
        )
    }
}

/// The members of an `implements` body, up to its `}`.
fn impl_members(p: &mut Parser<'_, '_>) -> List<AnyImplMember> {
    let mut members = Vec::new();
    while !p.cur.at(TokenKind::RBrace) && !p.cur.at_end() {
        if p.cur.at_set(TOP_LEVEL_KEYWORD) && !p.cur.at(TokenKind::Function) {
            break;
        }
        let member_mark = p.cur.mark();
        let member = p.attempt(|p| {
            let attrs = leading_block_attrs(p);
            if let Some(kw) = p.cur.eat::<kw::Function>() {
                Ok(AnyImplMember::Method(function_def(
                    p,
                    member_mark,
                    attrs,
                    kw,
                )))
            } else if !attrs.is_empty() {
                // An `implements` body has no attributes of its own.
                Err(p.unexpected_error("field or method definition expected in `implements` block"))
            } else if let Some(kw) = p.cur.eat::<ctx::Type>() {
                Ok(AnyImplMember::AssocType(AssocTypeDef::parse(p, kw)))
            } else if let Some(name) = p.cur.eat_set::<NameToken>(MEMBER_NAME) {
                // `name as field` links an interface field; otherwise a field.
                Ok(match p.cur.eat::<ctx::As>() {
                    Some(as_kw) => {
                        AnyImplMember::FieldLink(InterfaceFieldLink::parse(p, (name, as_kw)))
                    }
                    None => AnyImplMember::Field(Field::parse(p, name)),
                })
            } else {
                Ok(AnyImplMember::Bogus(p.bogus_one(
                    "field or method definition expected in `implements` block",
                )))
            }
        });
        members.push(match member {
            Ok(member) => member,
            Err(error) => AnyImplMember::Bogus(p.bogus_since(member_mark, error)),
        });
    }
    p.out.alloc_list(members)
}

/// `class_field,` after `interface_field as`.
impl Parse for InterfaceFieldLink {
    type Head = (Token![NameToken], Token!["as"]);
    fn parse(p: &mut Parser<'_, '_>, (iface_field, as_kw): Self::Head) -> Id<Self> {
        let mark = Mark::of(iface_field);
        let iface_field = p.name(iface_field);
        let as_kw = Ok(as_kw);
        let class_field = p.expect_name(MEMBER_NAME, "class field name");
        let sep = p.cur.eat_in::<MemberSep>();
        p.alloc(
            mark,
            InterfaceFieldLink {
                iface_field,
                as_kw,
                class_field,
                sep,
            },
        )
    }
}

/// `type Item extends Bound = Default,` in an interface: bound and default
/// optional.
impl Parse for AssocTypeDecl {
    type Head = Token!["type"];
    fn parse(p: &mut Parser<'_, '_>, kw: Self::Head) -> Id<Self> {
        let mark = Mark::of(kw);
        let name = expect_name(p, "associated type name");
        let bound = p.cur.eat::<kw::Extends>().map(|kw| {
            let ty = if is_at_type_start(p.cur) {
                any_type(p)
            } else {
                Err(p.missing("associated type bound"))
            };
            AssocBound { kw, ty }
        });
        let default = p.cur.eat::<punct::Equals>().map(|eq| {
            let ty = if is_at_type_start(p.cur) {
                any_type(p)
            } else {
                Err(p.missing("associated type binding"))
            };
            AssocDefault { eq, ty }
        });
        let sep = p.cur.eat_in::<MemberSep>();
        p.alloc(
            mark,
            AssocTypeDecl {
                kw,
                name,
                bound,
                default,
                sep,
            },
        )
    }
}

/// `type Item = T,` in an implementation: the binding is required, and an
/// interface-style `extends` bound is rejected.
impl Parse for AssocTypeDef {
    type Head = Token!["type"];
    fn parse(p: &mut Parser<'_, '_>, kw: Self::Head) -> Id<Self> {
        let mark = Mark::of(kw);
        let name = expect_name(p, "associated type name");
        if p.cur.at(TokenKind::Extends) {
            // Consumed for recovery and rejected: the tokens stay unowned and
            // the bound's nodes are discarded; its diagnostics stay.
            p.error_here("associated type bounds are only allowed on interface declarations");
            p.discard(|p| {
                p.cur.skip();
                if is_at_type_start(p.cur) {
                    let _rejected = any_type(p);
                } else {
                    p.unexpected("associated type bound");
                }
            });
        }
        let (eq, ty) = match p.cur.eat::<punct::Equals>() {
            Some(eq) => {
                let ty = if is_at_type_start(p.cur) {
                    any_type(p)
                } else {
                    Err(p.missing("associated type binding"))
                };
                (Ok(eq), ty)
            }
            None => {
                let missing = p.missing("associated type binding (`= Type`)");
                (Err(missing), Err(missing))
            }
        };
        let sep = p.cur.eat_in::<MemberSep>();
        p.alloc(
            mark,
            AssocTypeDef {
                kw,
                name,
                eq,
                ty,
                sep,
            },
        )
    }
}

// ----- type aliases, client values ---------------------------------------------------------------

/// `Name = T;` after a top-level `type`.
impl Parse for TypeAliasDef {
    type Head = Token!["type"];
    fn parse(p: &mut Parser<'_, '_>, kw: Self::Head) -> Id<Self> {
        let mark = Mark::of(kw);
        let name = expect_name(p, "type alias name");
        let eq = p.expect::<punct::Equals>();
        // Runtime type atoms are accepted here too; the declaration checker
        // reports that they have no lexical scope.
        let ty = any_type(p);
        reject_unmatched_greater(p);
        let semi = p.cur.eat::<punct::Semicolon>();
        p.alloc(
            mark,
            TypeAliasDef {
                kw,
                name,
                eq,
                ty,
                semi,
            },
        )
    }
}

/// `<expr>;` after `client Name =`.
impl Parse for ClientValueDef {
    type Head = (Token!["client"], Token![NameToken], Token!["="]);
    #[expect(deprecated, reason = "a stub stands in for a layer not parsed yet")]
    fn parse(p: &mut Parser<'_, '_>, (kw, name, eq): Self::Head) -> Id<Self> {
        let mark = Mark::of(kw);
        let name = p.name(name);
        let value = stubs::stub_expr(p, &[]);
        let semi = p.cur.eat::<punct::Semicolon>();
        p.alloc(
            mark,
            ClientValueDef {
                kw,
                name,
                eq,
                value,
                semi,
            },
        )
    }
}

// ----- removed items -------------------------------------------------------------------------------

/// The `{ … }` of a removed item, skipped without parsing. A body still open
/// where an item head starts a line was never closed: the skip stops there
/// and the missing `}` is reported, so the item after it is parsed. A
/// `type_builder` body nests `class` and `enum` heads by design, so it passes
/// no `resync` and is only ever skipped balanced.
fn skip_removed_body(p: &mut Parser<'_, '_>, resync: fn(Cursor<'_>) -> bool) {
    if !p.cur.skip_balanced_braces(resync) {
        p.unexpected("'}'");
    }
}

/// `<llm> Name { … }` after `client`: the config-block client of the legacy
/// world. `name` is the name when the caller already read it (looking for a
/// `client Name =` value).
fn removed_client(
    p: &mut Parser<'_, '_>,
    kw: Token![RemovedKw],
    name: Option<Id<Name>>,
) -> Id<RemovedItem> {
    let mark = Mark::of(kw);
    let head_mark = p.cur.mark();
    if name.is_none() && p.cur.at(TokenKind::Less) {
        p.cur.skip();
        if p.cur.at_ident() {
            p.cur.skip();
        }
        if p.cur.at(TokenKind::Greater) {
            p.cur.skip();
        } else {
            p.unexpected("'>'");
        }
    }
    let head = p.cur.range_from(head_mark);
    let name = name.or_else(|| {
        let name = p.eat_name(Ident::KINDS);
        if name.is_none() {
            p.unexpected("client name");
        }
        name
    });
    let body_mark = p.cur.mark();
    if p.cur.at(TokenKind::LBrace) {
        skip_removed_body(p, at_item_head);
    } else {
        p.unexpected("config block");
    }
    let body = p.cur.range_from(body_mark);
    p.alloc(
        mark,
        RemovedItem {
            kw,
            head,
            name,
            body,
        },
    )
}

/// `Name { … }` after `retry_policy`.
fn removed_retry_policy(p: &mut Parser<'_, '_>, kw: Token![RemovedKw]) -> Id<RemovedItem> {
    let mark = Mark::of(kw);
    let head = p.cur.range_from(p.cur.mark());
    let name = p.eat_name(Ident::KINDS);
    if name.is_none() {
        p.unexpected("retry policy name");
    }
    let body_mark = p.cur.mark();
    if p.cur.at(TokenKind::LBrace) {
        skip_removed_body(p, at_item_head);
    } else {
        p.unexpected("retry policy body");
    }
    let body = p.cur.range_from(body_mark);
    p.alloc(
        mark,
        RemovedItem {
            kw,
            head,
            name,
            body,
        },
    )
}

/// `generator name { … }`: swallowed opaquely so stale config never produces
/// interior errors; lowering raises the migration warning.
fn removed_generator(p: &mut Parser<'_, '_>, kw: Token![RemovedKw]) -> Id<RemovedItem> {
    let mark = Mark::of(kw);
    let head = p.cur.range_from(p.cur.mark());
    let name = p.eat_name(Ident::KINDS);
    let body_mark = p.cur.mark();
    if p.cur.at(TokenKind::LBrace) {
        skip_removed_body(p, at_item_head);
    }
    let body = p.cur.range_from(body_mark);
    p.alloc(
        mark,
        RemovedItem {
            kw,
            head,
            name,
            body,
        },
    )
}

/// `Name(params) "body"` after `template_string`.
#[expect(deprecated, reason = "a stub stands in for a layer not parsed yet")]
fn removed_template_string(p: &mut Parser<'_, '_>, kw: Token![RemovedKw]) -> Id<RemovedItem> {
    let mark = Mark::of(kw);
    let head = p.cur.range_from(p.cur.mark());
    let name = p.eat_name(Ident::KINDS);
    if name.is_none() {
        p.unexpected("template string name");
    }
    let body_mark = p.cur.mark();
    if p.cur.at(TokenKind::LParen) {
        p.cur.skip();
        p.cur.skip_until(KindSet::single(TokenKind::RParen));
        if p.cur.at(TokenKind::RParen) {
            p.cur.skip();
        } else {
            p.unexpected("')'");
        }
    }
    // The declaration-level removal diagnostic is more actionable than also
    // reporting the general hash-string error for its body.
    if let Some(token) = p.cur.eat_in::<StringToken>() {
        string_token_diagnostics(p, token.idx, StringSite::Removed);
    } else if p.cur.at(TokenKind::BacktickStart) {
        stubs::skip_backtick(&mut p.cur);
    } else {
        p.unexpected("template string body");
    }
    let body = p.cur.range_from(body_mark);
    p.alloc(
        mark,
        RemovedItem {
            kw,
            head,
            name,
            body,
        },
    )
}

/// `type_builder {` / `type_builder: {` at a position where a declaration or
/// config entry may start; a bare `type_builder` elsewhere is a name.
fn at_removed_type_builder(cur: Cursor<'_>) -> bool {
    cur.at(TokenKind::TypeBuilder)
        && match cur.kind(1) {
            Some(TokenKind::LBrace) => true,
            Some(TokenKind::Colon) => cur.nth_is(2, TokenKind::LBrace),
            _ => false,
        }
}

fn type_builder_removed_error(p: &Parser<'_, '_>, kw: TokenIdx) -> ParseError {
    ParseError::RemovedFeature {
        message: TYPE_BUILDER_REMOVED.to_string(),
        range: p.cur.token_range(kw),
    }
}

/// E0098 for the legacy `type_builder` block inside a body, after its
/// keyword, and the block consumed.
fn skip_removed_type_builder(p: &mut Parser<'_, '_>, kw: Token!["type_builder"]) -> Id<Bogus> {
    let error = type_builder_removed_error(p, kw.idx);
    p.cur.eat::<punct::Colon>();
    skip_removed_body(p, |_| false);
    p.bogus_since(Mark::of(kw), error)
}

fn removed_type_builder(p: &mut Parser<'_, '_>, kw: Token![RemovedKw]) -> Id<RemovedItem> {
    let error = type_builder_removed_error(p, kw.idx);
    p.out.report(error);
    let mark = Mark::of(kw);
    let head = p.cur.range_from(p.cur.mark());
    let body_mark = p.cur.mark();
    p.cur.eat::<punct::Colon>();
    skip_removed_body(p, |_| false);
    let body = p.cur.range_from(body_mark);
    p.alloc(
        mark,
        RemovedItem {
            kw,
            head,
            name: None,
            body,
        },
    )
}

/// `dynamic class Name { … }` / `dynamic enum Name { … }`.
fn at_removed_dynamic_def(cur: Cursor<'_>) -> bool {
    cur.at(TokenKind::Dynamic) && matches!(cur.kind(1), Some(TokenKind::Class | TokenKind::Enum))
}

/// `class Name { … }` after `dynamic`.
fn removed_dynamic_def(p: &mut Parser<'_, '_>, kw: Token![RemovedKw]) -> Id<RemovedItem> {
    let mark = Mark::of(kw);
    let keyword = p.cur.text_at(0).unwrap_or_default();
    let message = format!(
        "`dynamic {keyword}` definitions were removed; runtime type construction is `reflect` (BEP-066)"
    );
    let range = p.cur.token_range(kw.idx);
    p.removed(message, range);
    let head_mark = p.cur.mark();
    p.cur.skip(); // class / enum
    let head = p.cur.range_from(head_mark);
    let name = p.eat_name(Ident::KINDS);
    let body_mark = p.cur.mark();
    skip_removed_body(p, at_item_head);
    let body = p.cur.range_from(body_mark);
    p.alloc(
        mark,
        RemovedItem {
            kw,
            head,
            name,
            body,
        },
    )
}

/// `test Name { functions [...] … }`: a legacy config-block test.
fn at_removed_legacy_test(cur: Cursor<'_>) -> bool {
    cur.at(TokenKind::Test)
        && cur.nth_is_ident(1)
        && cur.nth_is(2, TokenKind::LBrace)
        && match cur.text_at(3) {
            Some("functions") => {
                matches!(cur.kind(4), Some(TokenKind::LBracket | TokenKind::Colon))
            }
            Some("args") => matches!(cur.kind(4), Some(TokenKind::LBrace | TokenKind::Colon)),
            _ if cur.nth_is(3, TokenKind::TypeBuilder) => {
                matches!(cur.kind(4), Some(TokenKind::LBrace | TokenKind::Colon))
            }
            _ => false,
        }
}

/// `Name { functions … }` after a legacy `test`.
fn removed_legacy_test(p: &mut Parser<'_, '_>, kw: Token![RemovedKw]) -> Id<RemovedItem> {
    let mark = Mark::of(kw);
    let start = p.cur.token_range(kw.idx);
    let name_range = p.cur.error_range();
    p.removed(
        "Legacy test declarations were removed. Use an expression-body test with a quoted name, for example: `test \"Name\" { assert.equal(Foo(...), expected) }`.",
        start.cover(name_range),
    );
    let head = p.cur.range_from(p.cur.mark());
    let name = p.eat_name(Ident::KINDS);
    let body_mark = p.cur.mark();
    skip_removed_body(p, at_item_head);
    let body = p.cur.range_from(body_mark);
    p.alloc(
        mark,
        RemovedItem {
            kw,
            head,
            name,
            body,
        },
    )
}

// ----- recovery -------------------------------------------------------------------------------------

/// `classs Foo { … }`: a misspelled keyword before a braced block.
fn try_recover_invalid_block(p: &mut Parser<'_, '_>) -> Option<Id<Bogus>> {
    if !(p.cur.at_ident() && p.cur.nth_is_ident(1) && p.cur.nth_is(2, TokenKind::LBrace)) {
        return None;
    }
    let keyword = p.cur.text_at(0).unwrap_or_default();
    let error = ParseError::InvalidSyntax {
        message: format!(
            "unknown keyword `{keyword}`; expected `class`, `enum`, `function`, `client`, `generator`, `test`, or `type`"
        ),
        range: p.cur.error_range(),
    };
    let mark = p.cur.mark();
    p.cur.skip();
    p.cur.skip();
    p.cur.skip_balanced_braces(at_item_head);
    Some(p.bogus_since(mark, error))
}

/// `typpe Name = …`: a misspelled `type`.
fn try_recover_invalid_type_alias(p: &mut Parser<'_, '_>) -> Option<Id<Bogus>> {
    if !(p.cur.at_ident() && p.cur.nth_is_ident(1) && p.cur.nth_is(2, TokenKind::Equals)) {
        return None;
    }
    let keyword = p.cur.text_at(0).unwrap_or_default();
    let error = ParseError::InvalidSyntax {
        message: format!(
            "unknown keyword `{keyword}`; did you mean `type`? usage: `type Name = expression`"
        ),
        range: p.cur.error_range(),
    };
    let mark = p.cur.mark();
    p.cur.skip();
    p.cur.skip();
    p.cur.skip();
    while !p.cur.at_end() && !p.cur.at_set(INVALID_ALIAS_END) {
        p.cur.skip();
    }
    Some(p.bogus_since(mark, error))
}

// ----- lookahead -------------------------------------------------------------------------------------

/// A `client<…>` declaration, as opposed to a class field named `client`.
fn looks_like_client_declaration_start(cur: Cursor<'_>) -> bool {
    cur.at(TokenKind::Client) && cur.nth_is(1, TokenKind::Less)
}

/// `interface Name … {` before any `}` or `;`, as opposed to a field named
/// `interface`.
fn looks_like_interface_declaration_start(cur: Cursor<'_>) -> bool {
    if !(cur.at(TokenKind::Interface) && cur.nth_is_ident(1)) {
        return false;
    }
    let mut i = cur.pos() + 2;
    while let Some(kind) = cur.kind_at_sig(i) {
        match kind {
            TokenKind::LBrace => return true,
            TokenKind::RBrace | TokenKind::Semicolon => return false,
            _ => i += 1,
        }
    }
    false
}

/// `let`, or a contextual `const` followed by a pattern-shaped token.
fn at_binding_intro_stmt(cur: Cursor<'_>) -> bool {
    cur.at(TokenKind::Let)
        || (cur.at(TokenKind::Const) && (cur.nth_is_ident(1) || cur.nth_is(1, TokenKind::LBracket)))
}

// ----- the file ---------------------------------------------------------------------------------------

pub(crate) fn source_file(p: &mut Parser<'_, '_>) -> Id<SourceFile> {
    let mark = p.cur.mark();
    let mut items = Vec::new();
    while !p.cur.at_end() {
        items.push(item(p));
    }
    diagnose_header_comments(p);
    diagnose_unclosed_block_comments(p);
    let items = p.out.alloc_list(items);
    p.alloc(mark, SourceFile { items })
}

fn item(p: &mut Parser<'_, '_>) -> AnyItem {
    let mark = p.cur.mark();
    // Attributes first; the token after them says what they belong to, and
    // attributes with nothing to attach to become one `Bogus`.
    let item = p.attempt(|p| {
        let attrs = leading_block_attrs(p);
        if let Some(kw) = p.cur.eat::<kw::Enum>() {
            Ok(AnyItem::Enum(enum_def(p, mark, attrs, kw)))
        } else if let Some(kw) = p.cur.eat::<kw::Class>() {
            Ok(AnyItem::Class(class_def(p, mark, attrs, kw)))
        } else if let Some(kw) = p.cur.eat::<kw::Interface>() {
            Ok(AnyItem::Interface(interface_def(p, mark, attrs, kw)))
        } else if let Some(kw) = p.cur.eat::<kw::Function>() {
            Ok(AnyItem::Function(function_def(p, mark, attrs, kw)))
        } else if attrs.is_empty() {
            Ok(unattributed_item(p))
        } else {
            Err(p.unexpected_error("top-level declaration"))
        }
    });
    match item {
        Ok(item) => item,
        Err(error) => AnyItem::Bogus(p.bogus_since(mark, error)),
    }
}

/// An item that takes no `@@attributes`, at its first token.
#[expect(deprecated, reason = "a stub stands in for a layer not parsed yet")]
fn unattributed_item(p: &mut Parser<'_, '_>) -> AnyItem {
    if let Some(kw) = p.cur.eat_in::<ImplementsKw>() {
        AnyItem::ImplementsFor(ImplementsFor::parse(p, kw))
    } else if let Some(kw) = p.cur.eat::<kw::Client>() {
        // `client Name = …` is a value; anything else after `client` is the
        // removed config block.
        match p.cur.eat_set::<NameToken>(Ident::KINDS) {
            Some(name) => match p.cur.eat::<punct::Equals>() {
                Some(eq) => AnyItem::ClientValue(ClientValueDef::parse(p, (kw, name, eq))),
                None => {
                    let name = p.name(name);
                    AnyItem::Removed(removed_client(p, kw.widen(), Some(name)))
                }
            },
            None => AnyItem::Removed(removed_client(p, kw.widen(), None)),
        }
    } else if let Some(kw) = p.cur.eat::<kw::Generator>() {
        AnyItem::Removed(removed_generator(p, kw.widen()))
    } else if at_removed_legacy_test(p.cur)
        && let Some(kw) = p.cur.eat::<kw::Test>()
    {
        AnyItem::Removed(removed_legacy_test(p, kw.widen()))
    } else if p.cur.at(TokenKind::Test) || p.cur.at(TokenKind::Testset) {
        AnyItem::Bogus(stubs::stub_test_item(p))
    } else if let Some(kw) = p.cur.eat::<kw::RetryPolicy>() {
        AnyItem::Removed(removed_retry_policy(p, kw.widen()))
    } else if let Some(kw) = p.cur.eat::<kw::TemplateString>() {
        AnyItem::Removed(removed_template_string(p, kw.widen()))
    } else if let Some(kw) = p.cur.eat::<ctx::Type>() {
        AnyItem::TypeAlias(TypeAliasDef::parse(p, kw))
    } else if at_binding_intro_stmt(p.cur) {
        AnyItem::Bogus(stubs::stub_top_level_stmt(p))
    } else if at_removed_type_builder(p.cur)
        && let Some(kw) = p.cur.eat::<ctx::TypeBuilder>()
    {
        AnyItem::Removed(removed_type_builder(p, kw.widen()))
    } else if at_removed_dynamic_def(p.cur)
        && let Some(kw) = p.cur.eat::<ctx::Dynamic>()
    {
        AnyItem::Removed(removed_dynamic_def(p, kw.widen()))
    } else if let Some(bogus) = try_recover_invalid_block(p) {
        AnyItem::Bogus(bogus)
    } else if let Some(bogus) = try_recover_invalid_type_alias(p) {
        AnyItem::Bogus(bogus)
    } else {
        // Nothing an item begins with: one `Bogus` over everything up to the
        // next item head or line-leading item keyword, reported once at its
        // first token, so stray text is one diagnostic and not one per token.
        let mark = p.cur.mark();
        let error = p.unexpected_error("top-level declaration");
        p.cur.skip();
        while !p.cur.at_end() && !at_item_start(p.cur) {
            p.cur.skip();
        }
        AnyItem::Bogus(p.bogus_since(mark, error))
    }
}

/// A `/*` with no `*/` swallowed the rest of the file as a comment: trivia to
/// the grammar, reported once here.
fn diagnose_unclosed_block_comments(p: &mut Parser<'_, '_>) {
    let tokens = p.cur.tokens();
    let unclosed: Vec<TokenIdx> = tokens
        .iter()
        .filter(|&idx| tokens.kind(idx) == TokenKind::UnclosedBlockComment)
        .collect();
    for idx in unclosed {
        let range = p.cur.token_range(idx);
        p.error(
            "unclosed block comment: expected `*/` before the end of the file",
            range,
        );
    }
}

/// A `//#` line is a statement; anywhere else it is an error. Until the
/// statement layer lands, bodies are `Bogus` placeholders and the header
/// comments inside them are not checked.
fn diagnose_header_comments(p: &mut Parser<'_, '_>) {
    let bogus_ranges = p.out.bogus_spans();
    let tokens = p.cur.tokens();
    let headers: Vec<TokenIdx> = tokens
        .iter()
        .filter(|&idx| tokens.kind(idx) == TokenKind::HeaderComment)
        .filter(|&idx| {
            !bogus_ranges
                .iter()
                .any(|r| r.start() <= idx && idx < r.end())
        })
        .collect();
    for idx in headers {
        let range = p.cur.token_range(idx);
        p.error(HEADER_COMMENT_MISPLACED, range);
    }
}
