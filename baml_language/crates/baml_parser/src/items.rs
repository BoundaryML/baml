//! Declarations: the file, items, their members, signatures, parameters,
//! generics and attributes. Section order follows the rowan parser so the
//! two read side by side.

use baml_syntax::{
    Token,
    nodes::{
        AnyAttrArg, AnyClassMember, AnyDoc, AnyEnumMember, AnyFunctionBody, AnyImplMember,
        AnyInterfaceMember, AnyItem, AnyLlmField, AnyStringLike, AssocBound, AssocDefault,
        AssocTypeDecl, AssocTypeDef, AttrArgs, Attribute, BacktickString, BlockAttr, BoundArm,
        ClassDef, ClientField, ClientValueDef, DefaultValue, Directive, DirectiveKind, DocLine,
        DottedName, EnumDef, EnumVariant, Field, FunctionDef, GenericParam, GenericParamBounds,
        GenericParamList, HeaderLine, ImplementsBlock, ImplementsFor, InterfaceDef,
        InterfaceFieldLink, JinjaArg, LetItem, LlmBody, MethodSig, Name, Param, ParamList,
        PathSegment, PromptField, RemovedItem, RequiresClause, ReturnType, SourceFile, StringLit,
        TestDef, ThrowsClause, ToolsField, TypeAliasDef, TypeAscription, TypeBindingStmt,
    },
    token::{
        Ident, KindSet, TokenIdx, TokenKind, TokenSet, ctx, doc, kw, lit, punct,
        sets::{ImplementsKw, LetIntroducer, MemberSep, NameToken, StringToken, TestKw},
    },
    tree::{AnyNodeId, Bogus, Id, List, Missing, TokenRange},
};

use crate::{
    cursor::{Cursor, Mark},
    error::ParseError,
    expr::{EXPR_NAME, Restrictions, at_expression_start, expr, with_clause},
    lookahead::at_binding_intro,
    parser::{Parse, Parser},
    stmts::{Semi, at_let_type, block, body_block, let_stmt},
    types::{MEMBER_NAME, any_type, is_at_type_start, reject_unmatched_greater},
};

const HASH_STRING_REMOVED: &str = "Hash string literals like `#\"...\"#` are no longer supported. Use quoted strings or backtick strings instead.";
const TYPE_BUILDER_REMOVED: &str =
    "`type_builder` blocks were removed; runtime type construction is `reflect` (BEP-066)";
const HEADER_COMMENT_MISPLACED: &str = "a `//#` header is a statement inside a block, or the title directly above a function; it is not allowed here";
const HEADER_COMMENT_REPEATED: &str = "a function has one `//#` title; this is a second one";

/// A keyword that always starts a fresh item at module scope; inside a body
/// it means a closing brace is missing.
pub(crate) const TOP_LEVEL_KEYWORD: KindSet = KindSet::new(&[
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
pub(crate) fn string_lit(p: &mut Parser<'_, '_>, site: StringSite) -> Option<Id<StringLit>> {
    let token = p.cur.eat_in::<StringToken>()?;
    string_token_diagnostics(p, token.idx, site);
    Some(p.alloc(Mark::of(token), StringLit { token }))
}

/// A string literal or a backtick template.
fn any_string_like(p: &mut Parser<'_, '_>, site: StringSite) -> Option<AnyStringLike> {
    if let Some(lit) = string_lit(p, site) {
        return Some(AnyStringLike::String(lit));
    }
    p.cur
        .eat::<lit::BacktickStart>()
        .map(|open| AnyStringLike::Backtick(BacktickString::parse(p, open)))
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
            Some(head) => Ok(dotted_name(
                p,
                head,
                Ident::KINDS,
                "attribute name segment after dot",
            )),
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
            Some(head) => Ok(dotted_name(
                p,
                head,
                BLOCK_ATTR_NAME,
                "attribute name segment after dot",
            )),
            None => Err(p.missing("attribute name")),
        };
        let args = attr_args(p, path.is_ok());
        p.alloc(Mark::of(at), BlockAttr { at, path, args })
    }
}

/// A `///` or `//baml:` line read before a declaration, not yet a node: it
/// becomes one once the declaration that owns it is known, or one `Bogus`
/// with its neighbours when no declaration follows — so no node is ever
/// allocated and then left unreferenced.
pub(crate) enum DocPart {
    Doc(Token![DocComment]),
    Directive(Token![DocToken], DirectiveKind),
    /// A `//#` line: a function's title, an error above anything else.
    Header(Token![HeaderComment]),
    /// A `//baml:` line naming no directive, or a malformed trace hook.
    Invalid(Token![DocToken], DocProblem),
}

/// Why a doc line is no directive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DocProblem {
    /// `//baml:<name>` with a name no directive has.
    UnknownDirective,
    /// `baml:$trace` with no `=a.b.c` hook path after it.
    MalformedTraceHook,
}

/// What a run of docs belongs to: decides what its directives may be.
#[derive(Debug, Clone, Copy)]
pub(crate) enum DocOwner<'a> {
    /// A function definition, named once its name has parsed: directives are
    /// its attributes, with one trace hook at most.
    Function { name: Option<&'a str> },
    /// Any other declaration or member: directives belong to function
    /// definitions only, so each one here is an error.
    Other,
}

const TRACE_HOOK_MISPLACED: &str = "Trace hooks can only be attached to function definitions.\nhelp: Place `/// baml:$trace=...` immediately before a function definition.";
const TRACE_PASSTHROUGH_MISPLACED: &str =
    "The internal trace_passthrough directive must immediately precede a function definition.";
const TRACE_HOOK_MALFORMED: &str = "Expected `/// baml:$trace=hook_name` with a function reference.\nhelp: Use a function name without call parentheses, for example `/// baml:$trace=trace.empty_span`.";

impl DocPart {
    fn token(&self) -> TokenIdx {
        match self {
            DocPart::Doc(token) => token.idx,
            DocPart::Header(token) => token.idx,
            DocPart::Directive(token, _) | DocPart::Invalid(token, _) => token.idx,
        }
    }
}

/// What precedes a declaration — `///` doc lines, `//baml:` directives and
/// `//#` header lines — in source order, parsed before anything decides what
/// they belong to: the caller dispatches on the token after them. A `@@`
/// attribute is never one of these: it is written inside the body of the
/// declaration it is for. A header is trivia to the cursor, so the ones
/// between the last token read and the next significant one are collected
/// before each doc line.
fn leading_docs(p: &mut Parser<'_, '_>) -> Vec<DocPart> {
    let mut parts = Vec::new();
    loop {
        parts.extend(p.cur.headers_ahead().map(DocPart::Header));
        if let Some(token) = p.cur.eat::<doc::DocComment>() {
            parts.push(doc_line(p, token));
        } else if let Some(token) = p.cur.eat::<doc::DirectiveComment>() {
            parts.push(directive(p, token));
        } else {
            return parts;
        }
    }
}

/// A `///` line after its token: a doc line, unless it is a `baml:$trace`
/// hook.
fn doc_line(p: &Parser<'_, '_>, token: Token![DocComment]) -> DocPart {
    let text = p.cur.text_of(token.idx);
    match DirectiveKind::trace_hook_value(text) {
        Some(value) => trace_hook(token.widen(), value),
        None => DocPart::Doc(token),
    }
}

/// A `//baml:` line after its token: the directive it names, or invalid when
/// it names none. `//baml:$trace=…` is the hook too.
fn directive(p: &Parser<'_, '_>, token: Token![DirectiveComment]) -> DocPart {
    let text = p.cur.text_of(token.idx);
    if let Some(value) = DirectiveKind::trace_hook_value(text) {
        return trace_hook(token.widen(), value);
    }
    let name = text.strip_prefix("//baml:").unwrap_or_default().trim_end();
    match DirectiveKind::named(name) {
        Some(kind) => DocPart::Directive(token.widen(), kind),
        None => DocPart::Invalid(token.widen(), DocProblem::UnknownDirective),
    }
}

/// `baml:$trace` with `value` after it: the hook when the value is `=a.b.c`.
fn trace_hook(token: Token![DocToken], value: &str) -> DocPart {
    if DirectiveKind::trace_hook_path(value).is_some() {
        return DocPart::Directive(token, DirectiveKind::Trace);
    }
    DocPart::Invalid(token, DocProblem::MalformedTraceHook)
}

/// The nodes of a declaration's docs, now that it owns them. A directive the
/// owner cannot take, a second trace hook, a line that is no directive, or a
/// `//#` header above anything but a function (or a second one above a
/// function) is a `Bogus` with its error, so a `Directive` or `HeaderLine`
/// node is always one its owner accepts.
fn alloc_docs(p: &mut Parser<'_, '_>, docs: Vec<DocPart>, owner: DocOwner<'_>) -> Vec<AnyDoc> {
    let mut hook_seen = false;
    let mut header_seen = false;
    docs.into_iter()
        .map(|part| {
            // The declaration's keyword has been read by now: each node
            // spans exactly its own line.
            let token = part.token();
            let end = TokenIdx(token.0 + 1);
            let text = p.cur.text_of(token).trim_end();
            if let DocPart::Header(header) = part {
                let accepted = matches!(owner, DocOwner::Function { .. }) && !header_seen;
                header_seen = true;
                if accepted {
                    return AnyDoc::Header(p.alloc_to(
                        Mark::of(header),
                        end,
                        HeaderLine { token: header },
                    ));
                }
                let range = p.cur.token_range(token);
                let message = if matches!(owner, DocOwner::Function { .. }) {
                    HEADER_COMMENT_REPEATED
                } else {
                    HEADER_COMMENT_MISPLACED
                };
                let error = ParseError::InvalidSyntax {
                    message: message.to_string(),
                    range,
                };
                return AnyDoc::Bogus(p.bogus_tokens(TokenRange::new(token, end), error));
            }
            let message = match owner {
                DocOwner::Function { name } => match part {
                    DocPart::Header(_) => unreachable!("headers were handled above"),
                    DocPart::Doc(token) => {
                        return AnyDoc::Doc(p.alloc_to(Mark::of(token), end, DocLine { token }));
                    }
                    DocPart::Directive(_, DirectiveKind::Trace) if hook_seen => format!(
                        "Function `{}` declares more than one trace hook.\nhelp: Keep one `/// baml:$trace=...` directive.",
                        name.unwrap_or_default()
                    ),
                    DocPart::Directive(token, kind) => {
                        hook_seen |= kind == DirectiveKind::Trace;
                        return AnyDoc::Directive(p.alloc_to(
                            Mark::of(token),
                            end,
                            Directive { token, kind },
                        ));
                    }
                    DocPart::Invalid(_, DocProblem::UnknownDirective) => {
                        format!("unknown directive `{text}`")
                    }
                    DocPart::Invalid(_, DocProblem::MalformedTraceHook) => match name {
                        Some(name) => format!(
                            "Invalid trace hook directive on function `{name}`.\n{TRACE_HOOK_MALFORMED}"
                        ),
                        None => format!("Invalid trace hook directive. {TRACE_HOOK_MALFORMED}"),
                    },
                },
                DocOwner::Other => match part {
                    DocPart::Header(_) => unreachable!("headers were handled above"),
                    DocPart::Doc(token) => {
                        return AnyDoc::Doc(p.alloc_to(Mark::of(token), end, DocLine { token }));
                    }
                    DocPart::Directive(_, DirectiveKind::Trace)
                    | DocPart::Invalid(_, DocProblem::MalformedTraceHook) => {
                        TRACE_HOOK_MISPLACED.to_string()
                    }
                    DocPart::Directive(_, DirectiveKind::TracePassthrough) => {
                        TRACE_PASSTHROUGH_MISPLACED.to_string()
                    }
                    DocPart::Directive(
                        _,
                        DirectiveKind::MutSelf
                        | DirectiveKind::Vm
                        | DirectiveKind::MutVm
                        | DirectiveKind::MayYield
                        | DirectiveKind::Fallible
                        | DirectiveKind::TaggedString
                        | DirectiveKind::LocalTwin,
                    ) => format!("`{text}` can only be attached to function definitions."),
                    DocPart::Invalid(_, DocProblem::UnknownDirective) => {
                        format!("unknown directive `{text}`")
                    }
                },
            };
            let range = p.cur.token_range(token);
            let error = ParseError::InvalidDirective { message, range };
            AnyDoc::Bogus(p.bogus_tokens(TokenRange::new(token, end), error))
        })
        .collect()
}

/// Docs that precede nothing they can document: one `Bogus` over their lines,
/// with the error; `None` when there are none. A run of `//#` headers alone
/// gets the header message.
fn reject_docs(p: &mut Parser<'_, '_>, docs: &[DocPart]) -> Option<Id<Bogus>> {
    let (first, last) = (docs.first()?.token(), docs.last()?.token());
    let tokens = TokenRange::new(first, TokenIdx(last.0 + 1));
    let range = p.cur.text_range(tokens);
    let error = if docs.iter().all(|doc| matches!(doc, DocPart::Header(_))) {
        ParseError::InvalidSyntax {
            message: HEADER_COMMENT_MISPLACED.to_string(),
            range,
        }
    } else {
        ParseError::InvalidDirective {
            message: "`///` doc comments and `//baml:` directives must directly precede the declaration they belong to; use `//` for a plain comment".to_string(),
            range,
        }
    };
    Some(p.bogus_tokens(tokens, error))
}

/// Where a declaration with `docs` begins: at its first doc line, else at
/// `token`.
fn prefixed_mark<S: TokenSet>(docs: &[DocPart], token: baml_syntax::token::Token<S>) -> Mark {
    docs.first()
        .map_or(Mark::of(token), |doc| Mark::at(doc.token()))
}

/// `b.c` after the first name `head` of `a.b.c`, each later segment one of
/// `segment`; a dot with no segment after it is reported as `expected`.
pub(crate) fn dotted_name(
    p: &mut Parser<'_, '_>,
    head: Token![NameToken],
    segment: KindSet,
    expected: &str,
) -> Id<DottedName> {
    let mark = Mark::of(head);
    let head = p.name(head);
    let segments = path_segments(p, |p| p.expect_name(segment, expected));
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
fn attribute_arg(p: &mut Parser<'_, '_>) -> Option<AnyAttrArg> {
    if let Some(lit) = string_lit(p, StringSite::Literal) {
        return Some(AnyAttrArg::String(lit));
    }
    if let Some(open) = p.cur.eat::<lit::BacktickStart>() {
        return Some(AnyAttrArg::Backtick(BacktickString::parse(p, open)));
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
    let Some(name) = p.eat_name(Ident::KINDS) else {
        p.unexpected("type parameter name");
        return None;
    };
    let bounds = p
        .cur
        .eat::<kw::Extends>()
        .map(|kw| GenericParamBounds::parse(p, kw));
    let mark = p.mark_of(AnyNodeId::of(name));
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

/// Whether a parameter's type annotation is required (a declaration) or
/// optional (a lambda, whose parameter types may be inferred).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Annotation {
    Required,
    Optional,
}

/// `a: A, b: B = default)` after the `(` of a declaration.
impl Parse for ParamList {
    type Head = Token!["("];
    fn parse(p: &mut Parser<'_, '_>, l: Self::Head) -> Id<Self> {
        param_list(p, l, Annotation::Required)
    }
}

/// `a: A, b = default)` after the `(`.
pub(crate) fn param_list(
    p: &mut Parser<'_, '_>,
    l: Token!["("],
    annotation: Annotation,
) -> Id<ParamList> {
    let params = p.sep_list(TokenKind::RParen, |p| param(p, annotation));
    let r = p.expect::<punct::RParen>();
    p.alloc(Mark::of(l), ParamList { l, params, r })
}

/// `name: T = default`; `self` takes no type. The colon is optional, but
/// without it the type must share the name's line.
fn param(p: &mut Parser<'_, '_>, annotation: Annotation) -> Option<Id<Param>> {
    let mark = p.cur.mark();
    reject_reserved_trace_parameter(p);
    let is_self = p.cur.at(TokenKind::SelfValue);
    let name = if let Some(name) = p.eat_name(EXPR_NAME) {
        Ok(name)
    } else if annotation == Annotation::Required && is_at_type_start(p.cur) {
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
        match annotation {
            Annotation::Required => {
                let ty = if is_at_type_start(p.cur) && (!newline_before_type || colon.is_some()) {
                    any_type(p)
                } else {
                    Err(p.missing("type annotation"))
                };
                reject_unmatched_greater(p);
                Some(p.alloc(ascription_mark, TypeAscription { colon, ty }))
            }
            Annotation::Optional => colon.map(|colon| {
                let ty = any_type(p);
                reject_unmatched_greater(p);
                p.alloc(
                    ascription_mark,
                    TypeAscription {
                        colon: Some(colon),
                        ty,
                    },
                )
            }),
        }
    };
    // `self` consumes a default too, so lowering can reject it specifically.
    let default = p.cur.eat::<punct::Equals>().map(|eq| {
        let value = if at_expression_start(p.cur) {
            expr(p, Restrictions::empty())
        } else {
            Err(p.missing("expression"))
        };
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
/// `function name<G>(params) -> R throws E`. Its docs are not here: what
/// they may hold depends on whether a body follows, so the caller owns them
/// once it knows.
struct Signature {
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

impl Signature {
    /// The docs of the definition this signature heads, reported by its
    /// name where a trace hook is malformed or repeated.
    fn function_docs(&self, p: &mut Parser<'_, '_>, docs: Vec<DocPart>) -> List<AnyDoc> {
        let name = self
            .name
            .ok()
            .map(|id| p.cur.text_of(p.out.get(id).token.idx));
        let docs = alloc_docs(p, docs, DocOwner::Function { name });
        p.out.alloc_list(docs)
    }
}

/// After `function`.
fn signature(p: &mut Parser<'_, '_>, kw: Token!["function"]) -> Signature {
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
        kw,
        name,
        generics,
        params,
        ret,
        throws,
        allow_llm_body,
    }
}

/// `… { body }` after `function` — a function, method or default method,
/// whose body is required. `mark` is where its docs began.
fn function_def(
    p: &mut Parser<'_, '_>,
    mark: Mark,
    docs: Vec<DocPart>,
    kw: Token!["function"],
) -> Id<FunctionDef> {
    let sig = signature(p, kw);
    let docs = sig.function_docs(p, docs);
    let body = match p.cur.eat::<punct::LBrace>() {
        Some(l) => Ok(function_body(p, l, sig.allow_llm_body)),
        None => Err(p.missing("function body")),
    };
    p.alloc(
        mark,
        FunctionDef {
            docs,
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
    docs: Vec<DocPart>,
    kw: Token!["function"],
) -> AnyInterfaceMember {
    let sig = signature(p, kw);
    if let Some(l) = p.cur.eat::<punct::LBrace>() {
        let docs = sig.function_docs(p, docs);
        let body = Ok(function_body(p, l, sig.allow_llm_body));
        return AnyInterfaceMember::Method(p.alloc(
            mark,
            FunctionDef {
                docs,
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
    // A required method is no definition: nothing reads a directive on it.
    let docs = alloc_docs(p, docs, DocOwner::Other);
    let docs = p.out.alloc_list(docs);
    let semi = p.cur.eat::<punct::Semicolon>();
    AnyInterfaceMember::MethodSig(p.alloc(
        mark,
        MethodSig {
            docs,
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
fn function_body(p: &mut Parser<'_, '_>, l: Token!["{"], allow_llm_body: bool) -> AnyFunctionBody {
    if allow_llm_body
        && p.cur
            .looks_like_llm_function_body_from(p.cur.sig_index(l.idx))
    {
        AnyFunctionBody::Llm(LlmBody::parse(p, l))
    } else {
        AnyFunctionBody::Expr(block(p, l))
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
    fn parse(p: &mut Parser<'_, '_>, kw: Self::Head) -> Id<Self> {
        let mark = Mark::of(kw);
        let colon = p.expect::<punct::Colon>();
        let value = if p.cur.at(TokenKind::RBrace) || p.cur.has_newline_ahead() || p.cur.at_end() {
            Err(p.missing("client value"))
        } else if at_expression_start(p.cur) {
            expr(p, Restrictions::STATEMENT)
        } else {
            Err(p.missing("expression"))
        };
        p.alloc(mark, ClientField { kw, colon, value })
    }
}

impl Parse for ToolsField {
    type Head = Token!["tools"];
    fn parse(p: &mut Parser<'_, '_>, kw: Self::Head) -> Id<Self> {
        let mark = Mark::of(kw);
        let colon = p.expect::<punct::Colon>();
        let value = if at_expression_start(p.cur) {
            expr(p, Restrictions::STATEMENT)
        } else {
            Err(p.missing("expression"))
        };
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

/// After `enum`, with the docs already parsed; `mark` is where they began.
fn enum_def(
    p: &mut Parser<'_, '_>,
    mark: Mark,
    docs: Vec<AnyDoc>,
    kw: Token!["enum"],
) -> Id<EnumDef> {
    {
        let docs = p.out.alloc_list(docs);
        let name = expect_name(p, "enum name");
        let l = match p.expect::<punct::LBrace>() {
            Ok(l) => l,
            Err(missing) => {
                let members = p.out.alloc_list(Vec::new());
                return p.alloc(
                    mark,
                    EnumDef {
                        docs,
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
                continue;
            }
            let docs = leading_docs(p);
            if p.cur.at(TokenKind::AtAt) {
                // The docs documented nothing; the attribute is the next member.
                members.extend(reject_docs(p, &docs).map(AnyEnumMember::Bogus));
                continue;
            }
            if !p.cur.at_set(TOP_LEVEL_KEYWORD)
                && let Some(name) = p.cur.eat_set::<NameToken>(Ident::KINDS)
            {
                members.push(AnyEnumMember::Variant(EnumVariant::parse(p, (docs, name))));
            } else {
                members.extend(reject_docs(p, &docs).map(AnyEnumMember::Bogus));
                // After trailing docs the body may be over: nothing to skip.
                if p.cur.at_set(TOP_LEVEL_KEYWORD) || p.cur.at(TokenKind::RBrace) || p.cur.at_end()
                {
                    break;
                }
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
                docs,
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
    type Head = (Vec<DocPart>, Token![NameToken]);
    fn parse(p: &mut Parser<'_, '_>, (docs, name): Self::Head) -> Id<Self> {
        let mark = prefixed_mark(&docs, name);
        let docs = alloc_docs(p, docs, DocOwner::Other);
        let docs = p.out.alloc_list(docs);
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
        p.alloc(
            mark,
            EnumVariant {
                docs,
                name,
                attrs,
                sep,
            },
        )
    }
}

// ----- classes -----------------------------------------------------------------------------

/// After `class`, with the docs already parsed; `mark` is where they began.
fn class_def(
    p: &mut Parser<'_, '_>,
    mark: Mark,
    docs: Vec<AnyDoc>,
    kw: Token!["class"],
) -> Id<ClassDef> {
    {
        let docs = p.out.alloc_list(docs);
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
                        docs,
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
            if class_body_ends(p.cur) {
                break;
            }
            // A `@@` attribute is the class's, wherever in the body it stands.
            if let Some(at) = p.cur.eat::<punct::AtAt>() {
                members.push(AnyClassMember::BlockAttr(BlockAttr::parse(p, at)));
                continue;
            }
            let member_mark = p.cur.mark();
            let docs = leading_docs(p);
            if p.cur.at(TokenKind::AtAt) {
                // The docs documented nothing; the attribute is the next member.
                members.extend(reject_docs(p, &docs).map(AnyClassMember::Bogus));
                continue;
            }
            if let Some(kw) = p.cur.eat::<kw::Function>() {
                members.push(AnyClassMember::Method(function_def(
                    p,
                    member_mark,
                    docs,
                    kw,
                )));
                continue;
            }
            if let Some(kw) = p.cur.eat_in::<ImplementsKw>() {
                members.push(AnyClassMember::Implements(ImplementsBlock::parse(
                    p,
                    (docs, kw),
                )));
            } else if class_body_ends(p.cur) {
                // The docs preceded the next item, not a member.
                members.extend(reject_docs(p, &docs).map(AnyClassMember::Bogus));
                break;
            } else if let Some(name) = p.cur.eat_set::<NameToken>(MEMBER_NAME) {
                members.push(AnyClassMember::Field(Field::parse(p, (docs, name))));
            } else {
                members.extend(reject_docs(p, &docs).map(AnyClassMember::Bogus));
                // After trailing docs the body may be over: nothing to skip.
                if p.cur.at(TokenKind::RBrace) || p.cur.at_end() {
                    break;
                }
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
                docs,
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
    type Head = (Vec<DocPart>, Token![NameToken]);
    fn parse(p: &mut Parser<'_, '_>, (docs, name): Self::Head) -> Id<Self> {
        let mark = prefixed_mark(&docs, name);
        let docs = alloc_docs(p, docs, DocOwner::Other);
        let docs = p.out.alloc_list(docs);
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
                docs,
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

/// After `interface`, with the docs already parsed; `mark` is where they
/// began.
fn interface_def(
    p: &mut Parser<'_, '_>,
    mark: Mark,
    docs: Vec<AnyDoc>,
    kw: Token!["interface"],
) -> Id<InterfaceDef> {
    {
        let docs = p.out.alloc_list(docs);
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
                        docs,
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
            if let Some(at) = p.cur.eat::<punct::AtAt>() {
                members.push(AnyInterfaceMember::BlockAttr(BlockAttr::parse(p, at)));
                continue;
            }
            let member_mark = p.cur.mark();
            let docs = leading_docs(p);
            if p.cur.at(TokenKind::AtAt) {
                // The docs documented nothing; the attribute is the next member.
                members.extend(reject_docs(p, &docs).map(AnyInterfaceMember::Bogus));
                continue;
            }
            if let Some(kw) = p.cur.eat::<kw::Function>() {
                members.push(interface_method(p, member_mark, docs, kw));
                continue;
            }
            if let Some(kw) = p.cur.eat::<ctx::Type>() {
                members.push(AnyInterfaceMember::AssocType(AssocTypeDecl::parse(
                    p,
                    (docs, kw),
                )));
            } else if let Some(name) = p.cur.eat_set::<NameToken>(Ident::KINDS) {
                members.push(AnyInterfaceMember::Field(Field::parse(p, (docs, name))));
            } else {
                members.extend(reject_docs(p, &docs).map(AnyInterfaceMember::Bogus));
                // After trailing docs the body may be over: nothing to skip.
                if p.cur.at_set(TOP_LEVEL_KEYWORD) || p.cur.at(TokenKind::RBrace) || p.cur.at_end()
                {
                    break;
                }
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
                docs,
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
        // `is_at_type_start` admits a `<` that `any_type` then rejects (no
        // function type follows): a reported `Missing` either way.
        let types = if is_at_type_start(p.cur)
            && let Ok(first) = any_type(p)
        {
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
    type Head = (Vec<DocPart>, Token![ImplementsKw]);
    fn parse(p: &mut Parser<'_, '_>, (docs, kw): Self::Head) -> Id<Self> {
        let mark = prefixed_mark(&docs, kw);
        let docs = alloc_docs(p, docs, DocOwner::Other);
        let docs = p.out.alloc_list(docs);
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
                        docs,
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
                docs,
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
    type Head = (Vec<DocPart>, Token![ImplementsKw]);
    fn parse(p: &mut Parser<'_, '_>, (docs, kw): Self::Head) -> Id<Self> {
        let mark = prefixed_mark(&docs, kw);
        let docs = alloc_docs(p, docs, DocOwner::Other);
        let docs = p.out.alloc_list(docs);
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
                        docs,
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
                docs,
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
        let before = p.cur.pos();
        let member = p.attempt(|p| {
            let docs = leading_docs(p);
            // An `implements` body has no attributes of its own: the docs
            // before a `@@` documented nothing, and the `@@` is skipped so
            // the loop moves on.
            if p.cur.at(TokenKind::AtAt) {
                if let Some(bogus) = reject_docs(p, &docs) {
                    return Ok(AnyImplMember::Bogus(bogus));
                }
                return Ok(AnyImplMember::Bogus(p.bogus_one(
                    "field or method definition expected in `implements` block",
                )));
            }
            if let Some(kw) = p.cur.eat::<kw::Function>() {
                return Ok(AnyImplMember::Method(function_def(
                    p,
                    member_mark,
                    docs,
                    kw,
                )));
            }
            if let Some(kw) = p.cur.eat::<ctx::Type>() {
                Ok(AnyImplMember::AssocType(AssocTypeDef::parse(p, (docs, kw))))
            } else if !p.cur.at_set(TOP_LEVEL_KEYWORD)
                && let Some(name) = p.cur.eat_set::<NameToken>(MEMBER_NAME)
            {
                // `name as field` links an interface field; otherwise a field.
                Ok(match p.cur.eat::<ctx::As>() {
                    Some(as_kw) => {
                        AnyImplMember::FieldLink(InterfaceFieldLink::parse(p, (docs, name, as_kw)))
                    }
                    None => AnyImplMember::Field(Field::parse(p, (docs, name))),
                })
            } else if let Some(bogus) = reject_docs(p, &docs) {
                Ok(AnyImplMember::Bogus(bogus))
            } else if p.cur.at_set(TOP_LEVEL_KEYWORD) {
                Err(p.unexpected_error("field or method definition expected in `implements` block"))
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
        // A member that consumed nothing must not be read again here.
        if p.cur.pos() == before && !p.cur.at(TokenKind::RBrace) && !p.cur.at_end() {
            p.cur.skip();
        }
    }
    p.out.alloc_list(members)
}

/// `class_field,` after `interface_field as`.
impl Parse for InterfaceFieldLink {
    type Head = (Vec<DocPart>, Token![NameToken], Token!["as"]);
    fn parse(p: &mut Parser<'_, '_>, (docs, iface_field, as_kw): Self::Head) -> Id<Self> {
        let mark = prefixed_mark(&docs, iface_field);
        let docs = alloc_docs(p, docs, DocOwner::Other);
        let docs = p.out.alloc_list(docs);
        let iface_field = p.name(iface_field);
        let as_kw = Ok(as_kw);
        let class_field = p.expect_name(MEMBER_NAME, "class field name");
        let sep = p.cur.eat_in::<MemberSep>();
        p.alloc(
            mark,
            InterfaceFieldLink {
                docs,
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
    type Head = (Vec<DocPart>, Token!["type"]);
    fn parse(p: &mut Parser<'_, '_>, (docs, kw): Self::Head) -> Id<Self> {
        let mark = prefixed_mark(&docs, kw);
        let docs = alloc_docs(p, docs, DocOwner::Other);
        let docs = p.out.alloc_list(docs);
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
                docs,
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
    type Head = (Vec<DocPart>, Token!["type"]);
    fn parse(p: &mut Parser<'_, '_>, (docs, kw): Self::Head) -> Id<Self> {
        let mark = prefixed_mark(&docs, kw);
        let docs = alloc_docs(p, docs, DocOwner::Other);
        let docs = p.out.alloc_list(docs);
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
                docs,
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
    type Head = (Vec<DocPart>, Token!["type"]);
    fn parse(p: &mut Parser<'_, '_>, (docs, kw): Self::Head) -> Id<Self> {
        let mark = prefixed_mark(&docs, kw);
        let docs = alloc_docs(p, docs, DocOwner::Other);
        let docs = p.out.alloc_list(docs);
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
                docs,
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
    type Head = (
        Vec<DocPart>,
        Token!["client"],
        Token![NameToken],
        Token!["="],
    );
    fn parse(p: &mut Parser<'_, '_>, (docs, kw, name, eq): Self::Head) -> Id<Self> {
        let mark = prefixed_mark(&docs, kw);
        let docs = alloc_docs(p, docs, DocOwner::Other);
        let docs = p.out.alloc_list(docs);
        let name = p.name(name);
        let value = if at_expression_start(p.cur) {
            expr(p, Restrictions::STATEMENT)
        } else {
            Err(p.missing("expression"))
        };
        let semi = p.cur.eat::<punct::Semicolon>();
        p.alloc(
            mark,
            ClientValueDef {
                docs,
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
    } else if let Some(open) = p.cur.eat::<lit::BacktickStart>() {
        // Read for its extent only: the removal diagnostic covers it.
        p.probe(|p| {
            let _body = BacktickString::parse(p, open);
        });
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

// ----- tests ---------------------------------------------------------------------------------------------

/// Where a `test`/`testset` is written: at the top level, or as a statement
/// in a testset's body — where its name may be a computed expression, so
/// only the item position reports a bare word as a mistaken unquoted name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TestPosition {
    Item,
    Stmt,
}

/// `"name" with runner { body }` after `test`/`testset`.
pub(crate) fn test_def(
    p: &mut Parser<'_, '_>,
    docs: Vec<DocPart>,
    kw: Token![TestKw],
    position: TestPosition,
) -> Id<TestDef> {
    let mark = prefixed_mark(&docs, kw);
    let docs = alloc_docs(p, docs, DocOwner::Other);
    let docs = p.out.alloc_list(docs);
    let what = if p.cur.tokens().kind(kw.idx) == TokenKind::Test {
        "test"
    } else {
        "testset"
    };
    // A bare identifier can never resolve at the top level: report the fix
    // instead of a downstream "unresolved name".
    if position == TestPosition::Item
        && p.cur.at(TokenKind::Word)
        && (p.cur.nth_is(1, TokenKind::LBrace) || p.cur.nth_is(1, TokenKind::With))
    {
        let name = p.cur.text_at(0).unwrap_or_default();
        p.error_here(format!(
            "{what} names must be quoted strings: `{what} \"{name}\"`"
        ));
    }
    let r = Restrictions::STATEMENT.before_body();
    let name = if at_expression_start(p.cur) {
        expr(p, r)
    } else {
        Err(p.missing("expression"))
    };
    let with = p.cur.eat::<ctx::With>().map(|kw| with_clause(p, kw, r));
    let body = body_block(p, &format!("{what} body"));
    p.alloc(
        mark,
        TestDef {
            docs,
            kw,
            name,
            with,
            body,
        },
    )
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
    // The file starts where its first item does (a leading `//#` title
    // included), not at the first significant token.
    let mark = items.first().map_or(mark, |item| p.mark_of_any(item));
    let items = p.out.alloc_list(items);
    p.alloc(mark, SourceFile { items })
}

fn item(p: &mut Parser<'_, '_>) -> AnyItem {
    let mark = p.cur.mark();
    // Docs first; the token after them says what they belong to, and docs
    // with nothing to document become one `Bogus`.
    let item = p.attempt(|p| {
        let docs = leading_docs(p);
        // A `//#` title is trivia to the cursor: an item that begins with one
        // starts there, not at its first significant token.
        let mark = docs.first().map_or(mark, |doc| Mark::at(doc.token()));
        if p.cur.at(TokenKind::AtAt) {
            // A `@@` attribute has no position at the top level: it is written
            // inside the body of the declaration it is for. The docs before it
            // documented nothing and are their own `Bogus`; the attribute is
            // the next item.
            if let Some(bogus) = reject_docs(p, &docs) {
                return Ok(AnyItem::Bogus(bogus));
            }
            return Ok(AnyItem::Bogus(misplaced_block_attr(p)));
        }
        if let Some(kw) = p.cur.eat::<kw::Enum>() {
            let docs = alloc_docs(p, docs, DocOwner::Other);
            Ok(AnyItem::Enum(enum_def(p, mark, docs, kw)))
        } else if let Some(kw) = p.cur.eat::<kw::Class>() {
            let docs = alloc_docs(p, docs, DocOwner::Other);
            Ok(AnyItem::Class(class_def(p, mark, docs, kw)))
        } else if let Some(kw) = p.cur.eat::<kw::Interface>() {
            let docs = alloc_docs(p, docs, DocOwner::Other);
            Ok(AnyItem::Interface(interface_def(p, mark, docs, kw)))
        } else if let Some(kw) = p.cur.eat::<kw::Function>() {
            Ok(AnyItem::Function(function_def(p, mark, docs, kw)))
        } else {
            Ok(unattributed_item(p, docs))
        }
    });
    match item {
        Ok(item) => item,
        Err(error) => AnyItem::Bogus(p.bogus_since(mark, error)),
    }
}

/// A `@@` attribute outside every body, at its `@@`: read for its extent and
/// reported as one `Bogus`.
fn misplaced_block_attr(p: &mut Parser<'_, '_>) -> Id<Bogus> {
    let mark = p.cur.mark();
    let Some(at) = p.cur.eat::<punct::AtAt>() else {
        unreachable!("called at `@@`");
    };
    p.probe(|p| {
        BlockAttr::parse(p, at);
    });
    let range = p.cur.text_range(p.cur.range_from(mark));
    p.bogus_since(
        mark,
        ParseError::InvalidSyntax {
            message: "a `@@` attribute is written inside the body of the class, enum or interface it describes".to_string(),
            range,
        },
    )
}

/// An item at its first token, with the docs read before it. Docs before a form that cannot be documented (a removed
/// item, a `let type`, a recovery) are one `Bogus` item of their own, and the
/// form is the next item.
fn unattributed_item(p: &mut Parser<'_, '_>, docs: Vec<DocPart>) -> AnyItem {
    if let Some(kw) = p.cur.eat_in::<ImplementsKw>() {
        return AnyItem::ImplementsFor(ImplementsFor::parse(p, (docs, kw)));
    } else if (docs.is_empty() || at_client_value(p.cur))
        && let Some(kw) = p.cur.eat::<kw::Client>()
    {
        // `client Name = …` is a value; anything else after `client` is the
        // removed config block.
        return match p.cur.eat_set::<NameToken>(Ident::KINDS) {
            Some(name) => match p.cur.eat::<punct::Equals>() {
                Some(eq) => AnyItem::ClientValue(ClientValueDef::parse(p, (docs, kw, name, eq))),
                None => {
                    debug_assert!(docs.is_empty(), "`at_client_value` saw `client Name =`");
                    let name = p.name(name);
                    AnyItem::Removed(removed_client(p, kw.widen(), Some(name)))
                }
            },
            None => {
                debug_assert!(docs.is_empty(), "`at_client_value` saw `client Name =`");
                AnyItem::Removed(removed_client(p, kw.widen(), None))
            }
        };
    } else if !at_removed_legacy_test(p.cur)
        && let Some(kw) = p.cur.eat_in::<TestKw>()
    {
        return AnyItem::Test(test_def(p, docs, kw, TestPosition::Item));
    } else if let Some(kw) = p.cur.eat::<ctx::Type>() {
        return AnyItem::TypeAlias(TypeAliasDef::parse(p, (docs, kw)));
    } else if !at_let_type(p.cur)
        && at_binding_intro(p.cur)
        && let Some(introducer) = p.cur.eat_in::<LetIntroducer>()
    {
        return AnyItem::Let(let_item(p, docs, introducer));
    }
    if let Some(bogus) = reject_docs(p, &docs) {
        return AnyItem::Bogus(bogus);
    }
    if let Some(kw) = p.cur.eat::<kw::Client>() {
        // A removed `client<llm> Name { … }` block; its docs were rejected above.
        return match p.cur.eat_set::<NameToken>(Ident::KINDS) {
            Some(name) => {
                let name = p.name(name);
                AnyItem::Removed(removed_client(p, kw.widen(), Some(name)))
            }
            None => AnyItem::Removed(removed_client(p, kw.widen(), None)),
        };
    }
    if let Some(kw) = p.cur.eat::<kw::Generator>() {
        AnyItem::Removed(removed_generator(p, kw.widen()))
    } else if at_removed_legacy_test(p.cur)
        && let Some(kw) = p.cur.eat::<kw::Test>()
    {
        AnyItem::Removed(removed_legacy_test(p, kw.widen()))
    } else if let Some(kw) = p.cur.eat::<kw::RetryPolicy>() {
        AnyItem::Removed(removed_retry_policy(p, kw.widen()))
    } else if let Some(kw) = p.cur.eat::<kw::TemplateString>() {
        AnyItem::Removed(removed_template_string(p, kw.widen()))
    } else if at_let_type(p.cur)
        && let Some(let_kw) = p.cur.eat::<kw::Let>()
        && let Some(type_kw) = p.cur.eat::<ctx::Type>()
    {
        // Only a body can bind a runtime type; lowering reports it here.
        AnyItem::TypeBinding(TypeBindingStmt::parse(p, (let_kw, type_kw)))
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

/// Whether a class body is missing its `}`: a top-level keyword (other than
/// the members a class may hold) starts here. `interface` and `client` are
/// also member names, so their declaration shapes decide.
fn class_body_ends(cur: Cursor<'_>) -> bool {
    let recover_top_level_item = if cur.at(TokenKind::Interface) {
        looks_like_interface_declaration_start(cur)
    } else if cur.at(TokenKind::Client) {
        looks_like_client_declaration_start(cur)
    } else {
        cur.at_set(TOP_LEVEL_KEYWORD)
    };
    recover_top_level_item
        && !cur.at(TokenKind::Function)
        && !cur.at(TokenKind::Implements)
        && !cur.at(TokenKind::Implement)
}

/// A file-scope `let`/`const` after its introducer, with its docs.
fn let_item(
    p: &mut Parser<'_, '_>,
    docs: Vec<DocPart>,
    introducer: Token![LetIntroducer],
) -> Id<LetItem> {
    let mark = prefixed_mark(&docs, introducer);
    let docs = alloc_docs(p, docs, DocOwner::Other);
    let docs = p.out.alloc_list(docs);
    let stmt = let_stmt(p, introducer, Semi::Own);
    p.alloc(mark, LetItem { docs, stmt })
}

/// `client Name =`: a client value, the one `client` form that takes docs.
fn at_client_value(cur: Cursor<'_>) -> bool {
    cur.at(TokenKind::Client) && cur.nth_is_ident(1) && cur.nth_is(2, TokenKind::Equals)
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

/// A `//#` line is a statement in a block or a function's title; anywhere
/// else it is an error. One inside a `Bogus` — recovered text, or a template
/// interior not parsed yet — is not checked.
fn diagnose_header_comments(p: &mut Parser<'_, '_>) {
    let bogus_ranges = p.out.bogus_spans();
    let mut owned = p.out.owned_tokens::<baml_syntax::nodes::HeaderStmt>();
    owned.extend(p.out.owned_tokens::<baml_syntax::nodes::HeaderLine>());
    let tokens = p.cur.tokens();
    let headers: Vec<TokenIdx> = tokens
        .iter()
        .filter(|&idx| tokens.kind(idx) == TokenKind::HeaderComment)
        .filter(|idx| !owned.contains(idx))
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
