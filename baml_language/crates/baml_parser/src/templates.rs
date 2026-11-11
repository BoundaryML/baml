//! Backtick templates ([`BacktickString`]): text chunks, `${ … }`
//! interpolations (block bodies), and the structural `${for …}`/`${if …}`
//! blocks with their bodies nested inside. A block tag that cannot belong
//! where it is — a stray `${endfor}`, a second `${else}` — is a `Bogus` part
//! with the diagnostic lowering used to report for it; a wrong end tag closes
//! the innermost block and is reported as a mismatch, as lowering did.
//!
//! The lexer has already settled what is text and where the delimiters are:
//! the parser sees `BacktickStart`, `TemplateChunk`s, `${` openers whose
//! bodies are ordinary code up to their `}`, and the `BacktickEnd` (or not,
//! for an unclosed template).

use baml_syntax::{
    Token,
    nodes::{
        AnyExpr, AnyTemplatePart, BacktickString, TemplateElif, TemplateElse, TemplateEndTag,
        TemplateFor, TemplateForOpen, TemplateIf, TemplateIfOpen, TemplateInterp,
    },
    token::{TokenKind, kw, lit, punct, sets::EndTagKw},
    tree::{Bogus, Id, Missing},
};
use text_size::TextRange;

use crate::{
    cursor::{Cursor, Mark},
    error::ParseError,
    expr::{Restrictions, at_expression_start, expr, expr_from, if_after_cond},
    items::{RawStringFix, report_escape_issues_from},
    lookahead::{binding_intro_at, matching_close},
    parser::{Parse, Parser},
    stmts::{body_lines, for_header},
};

/// What a `${` opens, by the tokens right after it. `endfor`/`endif` are
/// contextual: only alone before the `}` are they tags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tag {
    For,
    If,
    ElseIf,
    Else,
    Endfor,
    Endif,
    /// An expression or statements.
    Interp,
}

fn tag_at(cur: Cursor<'_>) -> Tag {
    debug_assert!(cur.at(TokenKind::DollarLBrace));
    match cur.kind(1) {
        Some(TokenKind::For) => Tag::For,
        Some(TokenKind::If) => Tag::If,
        Some(TokenKind::Else) if cur.nth_is(2, TokenKind::If) => Tag::ElseIf,
        Some(TokenKind::Else) => Tag::Else,
        Some(TokenKind::Endfor) if cur.nth_is(2, TokenKind::RBrace) => Tag::Endfor,
        Some(TokenKind::Endif) if cur.nth_is(2, TokenKind::RBrace) => Tag::Endif,
        _ => Tag::Interp,
    }
}

/// Whose parts are being read: decides which tags end the run and which
/// are errors there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Body {
    /// The template itself: every closing tag is stray.
    Template,
    /// A `${for}` block: an end tag closes it, `${else}` is stray.
    For,
    /// An `${if}`/`${else if}` branch: `${else}`, `${else if}` and the end
    /// tags all end it.
    IfThen,
    /// The `${else}` branch: a further `${else}`/`${else if}` is out of
    /// order.
    IfElse,
}

/// The whole template after its opener.
impl Parse for BacktickString {
    type Head = Token![BacktickStart];
    fn parse(p: &mut Parser<'_, '_>, open: Self::Head) -> Id<Self> {
        let ticks = p.cur.text_of(open.idx).len();
        let parts = parts(p, Body::Template);
        let parts = p.out.alloc_list(parts);
        // An N-tick opener with no close runs to the end of the file (BEP-049
        // §8); the usual intent behind a bare ``` `` ``` is an empty string.
        let expected = if ticks >= 2 {
            format!(
                "Unclosed backtick string (expected {}); use \"\" for an empty string (BEP-049 §8)",
                "`".repeat(ticks)
            )
        } else {
            "Unclosed backtick string (expected `)".to_string()
        };
        // Reported at the opener: the close is missing where the template
        // began, not at the end of the file it ran to.
        let close = match p.cur.eat::<lit::BacktickEnd>() {
            Some(close) => Ok(close),
            None => {
                let range = p.cur.token_range(open.idx);
                Err(p.missing_because(expected, range))
            }
        };
        p.alloc(Mark::of(open), BacktickString { open, parts, close })
    }
}

/// The parts of `body` up to the tag that ends it, the template's close, or
/// the end of the input.
fn parts(p: &mut Parser<'_, '_>, body: Body) -> Vec<AnyTemplatePart> {
    let mut parts = Vec::new();
    while !p.cur.at_end() && !p.cur.at(TokenKind::BacktickEnd) {
        if let Some(token) = p.cur.eat::<lit::TemplateChunk>() {
            // Escapes are checked chunk by chunk: a chunk is one run of
            // literal text, and an escape never spans a `${`.
            let issues =
                baml_base::escape::backtick_string_literal_escape_issues(p.cur.text_of(token.idx));
            let start = p.cur.token_range(token.idx).start();
            report_escape_issues_from(p, start, issues, RawStringFix::Offered);
            parts.push(AnyTemplatePart::Chunk(token));
            continue;
        }
        if !p.cur.at(TokenKind::DollarLBrace) {
            // Code left over after an interpolation lost its `}` (reported
            // there): skipped up to the next part.
            p.cur.skip();
            continue;
        }
        let tag = tag_at(p.cur);
        let stray = match (tag, body) {
            (Tag::Endfor | Tag::Endif, Body::For | Body::IfThen | Body::IfElse)
            | (Tag::Else | Tag::ElseIf, Body::IfThen) => break,
            (Tag::Endfor, Body::Template) => Some("stray ${endfor} with no matching ${for}"),
            (Tag::Endif, Body::Template) => Some("stray ${endif} with no matching ${if}"),
            (Tag::Else, Body::Template | Body::For) => Some("stray ${else} outside an ${if} block"),
            (Tag::ElseIf, Body::Template | Body::For) => {
                Some("stray ${else if} outside an ${if} block")
            }
            (Tag::Else, Body::IfElse) => Some("duplicate ${else} in the same ${if} chain"),
            (Tag::ElseIf, Body::IfElse) => Some("${else if} after ${else} in the same ${if} chain"),
            (Tag::For | Tag::If | Tag::Interp, _) => None,
        };
        let Some(open) = p.cur.eat::<punct::DollarLBrace>() else {
            p.cur.skip();
            continue;
        };
        if let Some(message) = stray {
            parts.push(AnyTemplatePart::Bogus(stray_tag(p, open, message)));
        } else if let Some(kw) = p.cur.eat::<kw::For>() {
            parts.push(AnyTemplatePart::For(TemplateFor::parse(p, (open, kw))));
        } else {
            parts.push(interpolation(p, open));
        }
    }
    parts
}

/// After the `${` of a tag with no place here: the whole tag through its
/// `}` as a `Bogus`, reported with `message`.
fn stray_tag(p: &mut Parser<'_, '_>, open: Token!["${"], message: &str) -> Id<Bogus> {
    let mark = Mark::of(open);
    let close = matching_close(p.cur, p.cur.sig_index(open.idx));
    while let Some(kind) = p.cur.kind(0) {
        let past_close = close.is_some_and(|close| p.cur.pos() > close);
        if past_close || (close.is_none() && kind == TokenKind::BacktickEnd) {
            break;
        }
        p.cur.skip();
    }
    let range = p.cur.text_range(p.cur.range_from(mark));
    p.bogus_since(
        mark,
        ParseError::InvalidSyntax {
            message: message.to_string(),
            range,
        },
    )
}

/// After a `${` that is not a `for` or closing tag: an interpolation, or an
/// `${if c}` block tag — told apart after the condition is parsed: a `}`
/// closes a tag, a `{` opens an if-expression's then-block.
fn interpolation(p: &mut Parser<'_, '_>, open: Token!["${"]) -> AnyTemplatePart {
    let mark = Mark::of(open);
    // `${if let …}` is an if-let expression: there is no such tag.
    let first = if p.cur.at(TokenKind::If)
        && !binding_intro_at(p.cur, 1)
        && let Some(kw) = p.cur.eat::<kw::If>()
    {
        let cond = if at_expression_start(p.cur) {
            expr(p, Restrictions::BRACE_IS_BODY)
        } else {
            Err(p.missing("expression"))
        };
        if !p.cur.at(TokenKind::LBrace) {
            let close = p.expect::<punct::RBrace>();
            let open = p.alloc(
                mark,
                TemplateIfOpen {
                    open,
                    kw,
                    cond,
                    close,
                },
            );
            return AnyTemplatePart::If(template_if(p, mark, open));
        }
        let if_expr = AnyExpr::If(if_after_cond(
            p,
            Mark::of(kw),
            kw,
            cond,
            Restrictions::STATEMENT,
        ));
        Some(expr_from(p, if_expr, Restrictions::STATEMENT))
    } else {
        None
    };
    let (stmts, tail) = body_lines(p, first);
    let stmts = p.out.alloc_list(stmts);
    let close = p.expect::<punct::RBrace>();
    AnyTemplatePart::Interp(p.alloc(
        mark,
        TemplateInterp {
            open,
            stmts,
            tail,
            close,
        },
    ))
}

/// Which block an end tag is expected to close.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Block {
    For,
    If,
}

/// The block after its `${for`, through its `${endfor}`.
impl Parse for TemplateFor {
    type Head = <TemplateForOpen as Parse>::Head;
    fn parse(p: &mut Parser<'_, '_>, head: Self::Head) -> Id<Self> {
        let mark = Mark::of(head.0);
        let open = TemplateForOpen::parse(p, head);
        let open_range = p.cur.text_range(p.out.node_span(open));
        let body = parts(p, Body::For);
        let body = p.out.alloc_list(body);
        let close = end_tag(p, Block::For, open_range);
        p.alloc(mark, TemplateFor { open, body, close })
    }
}

/// `${for header}`: the tag opening a loop block.
impl Parse for TemplateForOpen {
    type Head = (Token!["${"], Token!["for"]);
    fn parse(p: &mut Parser<'_, '_>, (open, kw): Self::Head) -> Id<Self> {
        // Inside the tag a `{` cannot be a body, only the `}` ends it; the
        // bare iterable keeps a brace literal out of the way all the same.
        let header = Ok(for_header(p, Restrictions::BRACE_IS_BODY));
        let close = p.expect::<punct::RBrace>();
        p.alloc(
            Mark::of(open),
            TemplateForOpen {
                open,
                kw,
                header,
                close,
            },
        )
    }
}

/// `${else if c}` and the parts it guards.
impl Parse for TemplateElif {
    type Head = (Token!["${"], Token!["else"], Token!["if"]);
    fn parse(p: &mut Parser<'_, '_>, (open, else_kw, if_kw): Self::Head) -> Id<Self> {
        let cond = if at_expression_start(p.cur) {
            expr(p, Restrictions::BRACE_IS_BODY)
        } else {
            Err(p.missing("expression"))
        };
        let close = p.expect::<punct::RBrace>();
        let parts = parts(p, Body::IfThen);
        let parts = p.out.alloc_list(parts);
        p.alloc(
            Mark::of(open),
            TemplateElif {
                open,
                else_kw,
                if_kw,
                cond,
                close,
                parts,
            },
        )
    }
}

/// `${else}` and the parts it guards.
impl Parse for TemplateElse {
    type Head = (Token!["${"], Token!["else"]);
    fn parse(p: &mut Parser<'_, '_>, (open, kw): Self::Head) -> Id<Self> {
        let close = p.expect::<punct::RBrace>();
        let parts = parts(p, Body::IfElse);
        let parts = p.out.alloc_list(parts);
        p.alloc(
            Mark::of(open),
            TemplateElse {
                open,
                kw,
                close,
                parts,
            },
        )
    }
}

/// After the `${if c}` tag `open` at `mark`: the branches through `${endif}`.
fn template_if(p: &mut Parser<'_, '_>, mark: Mark, open: Id<TemplateIfOpen>) -> Id<TemplateIf> {
    let open_range = p.cur.text_range(p.out.node_span(open));
    let then_parts = parts(p, Body::IfThen);
    let then_parts = p.out.alloc_list(then_parts);
    let mut elifs = Vec::new();
    while p.cur.at(TokenKind::DollarLBrace)
        && tag_at(p.cur) == Tag::ElseIf
        && let Some(open) = p.cur.eat::<punct::DollarLBrace>()
        && let Some(else_kw) = p.cur.eat::<kw::Else>()
        && let Some(if_kw) = p.cur.eat::<kw::If>()
    {
        elifs.push(TemplateElif::parse(p, (open, else_kw, if_kw)));
    }
    let elifs = p.out.alloc_list(elifs);
    let else_part = if p.cur.at(TokenKind::DollarLBrace)
        && tag_at(p.cur) == Tag::Else
        && let Some(open) = p.cur.eat::<punct::DollarLBrace>()
        && let Some(kw) = p.cur.eat::<kw::Else>()
    {
        Some(TemplateElse::parse(p, (open, kw)))
    } else {
        None
    };
    let close = end_tag(p, Block::If, open_range);
    p.alloc(
        mark,
        TemplateIf {
            open,
            then_parts,
            elifs,
            else_part,
            close,
        },
    )
}

/// The end tag closing a block of kind `expected`, whose open tag spans
/// `open_range`. The other end tag closes the block too, with a mismatch
/// reported at it; no end tag at all leaves the block unclosed, reported at
/// its open tag.
fn end_tag(
    p: &mut Parser<'_, '_>,
    expected: Block,
    open_range: TextRange,
) -> Result<Id<TemplateEndTag>, Missing> {
    let unclosed = |p: &mut Parser<'_, '_>| {
        let message = match expected {
            Block::For => "unclosed ${for} block — expected ${endfor}",
            Block::If => "unclosed ${if} block — expected ${endif}",
        };
        p.missing_because(message, open_range)
    };
    if !(p.cur.at(TokenKind::DollarLBrace) && matches!(tag_at(p.cur), Tag::Endfor | Tag::Endif)) {
        return Err(unclosed(p));
    }
    let (Some(open), Some(kw)) = (
        p.cur.eat::<punct::DollarLBrace>(),
        p.cur.eat_in::<EndTagKw>(),
    ) else {
        return Err(unclosed(p));
    };
    let close = p.expect::<punct::RBrace>();
    let id = p.alloc(Mark::of(open), TemplateEndTag { open, kw, close });
    let closes_for = p.cur.tokens().kind(kw.idx) == TokenKind::Endfor;
    let mismatch = match (expected, closes_for) {
        (Block::For, false) => Some("${for} block closed by ${endif} — expected ${endfor}"),
        (Block::If, true) => Some("${if} block closed by ${endfor} — expected ${endif}"),
        (Block::For, true) | (Block::If, false) => None,
    };
    if let Some(message) = mismatch {
        let range = p.cur.text_range(p.out.node_span(id));
        p.error(message, range);
    }
    Ok(id)
}
