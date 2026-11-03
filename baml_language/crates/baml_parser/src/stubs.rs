//! Placeholders for the grammar layers not yet ported — expressions,
//! statements, patterns and templates. Each consumes the construct as a
//! balanced token run and returns a `Bogus` placeholder, so the declaration
//! layer can be built and checked against the rowan parser first. The
//! parity oracle excludes everything inside these ranges; every function here
//! is deleted by the unit that ports its grammar, and with the last one goes
//! `Sink::placeholder`.

#![expect(
    deprecated,
    reason = "the stubs are built on the placeholder sink entry"
)]

use baml_syntax::{
    Token,
    nodes::AnyExpr,
    ops::BinaryOp,
    token::{TokenIdx, TokenKind, punct},
    tree::{Bogus, Id, Missing},
};

use crate::{
    cursor::{Cursor, Mark},
    items::at_item_head,
    parser::Parser,
};

/// After a body's `{`: the rest of the balanced `{ … }`. An unclosed body
/// stops where an item head starts a line; the missing `}` is the statement
/// layer's to report.
pub(crate) fn stub_block(p: &mut Parser<'_, '_>, l: Token!["{"]) -> Id<Bogus> {
    let mut depth = 1u32;
    while let Some(kind) = p.cur.kind(0) {
        match kind {
            TokenKind::LBrace | TokenKind::DollarLBrace => depth += 1,
            TokenKind::RBrace => {
                depth -= 1;
                p.cur.skip();
                if depth == 0 {
                    break;
                }
                continue;
            }
            _ if p.cur.starts_line(0) && at_item_head(p.cur) => break,
            _ => {}
        }
        p.cur.skip();
    }
    let tokens = p.cur.range_from(Mark::of(l));
    p.out.placeholder(tokens)
}

/// At a backtick opener: consume the whole template (interpolations and
/// nested templates included).
pub(crate) fn skip_backtick(cur: &mut Cursor<'_>) {
    debug_assert!(cur.at(TokenKind::BacktickStart));
    let mut depth = 0u32;
    while let Some(kind) = cur.kind(0) {
        match kind {
            TokenKind::BacktickStart => depth += 1,
            TokenKind::BacktickEnd => {
                depth -= 1;
                cur.skip();
                if depth == 0 {
                    break;
                }
                continue;
            }
            _ => {}
        }
        cur.skip();
    }
}

/// At a backtick opener: the whole template as a placeholder.
pub(crate) fn stub_backtick(p: &mut Parser<'_, '_>) -> Id<Bogus> {
    let mark = p.cur.mark();
    skip_backtick(&mut p.cur);
    let tokens = p.cur.range_from(mark);
    p.out.placeholder(tokens)
}

/// Consume an expression's tokens (see [`stub_expr`]) without building anything.
pub(crate) fn skip_expr(p: &mut Parser<'_, '_>, stops: &[TokenKind]) {
    stub_run(&mut p.cur, true, stops);
}

/// An expression: balanced tokens up to a closing delimiter, a separator, one
/// of `stops`, or a line break that is not followed by a continuation (`.`,
/// an operator, a `catch`). `Missing` if nothing was consumed.
pub(crate) fn stub_expr(p: &mut Parser<'_, '_>, stops: &[TokenKind]) -> Result<AnyExpr, Missing> {
    let mark = p.cur.mark();
    stub_run(&mut p.cur, true, stops);
    if p.cur.consumed_since(mark) {
        let tokens = p.cur.range_from(mark);
        Ok(AnyExpr::Bogus(p.out.placeholder(tokens)))
    } else {
        Err(p.missing("expression"))
    }
}

/// Consume balanced tokens up to a closing delimiter at depth 0, one of
/// `stops`, a separator (`,`/`;`) when `separators_stop`, or a line break not
/// followed by a continuation.
fn stub_run(cur: &mut Cursor<'_>, separators_stop: bool, stops: &[TokenKind]) {
    let mark = cur.mark();
    let mut depth = 0u32;
    while let Some(kind) = cur.kind(0) {
        if depth == 0 {
            // A `{` that is a stop still opens an object literal when a
            // `name:` field follows it (`test "x" with R { t: 0.5 } { … }`).
            let object_literal =
                kind == TokenKind::LBrace && cur.nth_is_ident(1) && cur.nth_is(2, TokenKind::Colon);
            if (matches!(
                kind,
                TokenKind::RParen | TokenKind::RBracket | TokenKind::RBrace
            ) || (separators_stop && matches!(kind, TokenKind::Comma | TokenKind::Semicolon))
                || stops.contains(&kind))
                && !object_literal
            {
                break;
            }
            if cur.consumed_since(mark) && cur.has_newline_ahead() && !continues_expr(*cur) {
                break;
            }
        }
        match kind {
            TokenKind::LParen
            | TokenKind::LBracket
            | TokenKind::LBrace
            | TokenKind::DollarLBrace => depth += 1,
            TokenKind::RParen | TokenKind::RBracket | TokenKind::RBrace => {
                depth = depth.saturating_sub(1);
            }
            _ => {}
        }
        cur.skip();
    }
}

/// Whether the current token continues the expression before a line break: a
/// member access, an infix operator (`-` only when spaced, as `a\n-b` is two
/// expressions), or a postfix clause.
fn continues_expr(cur: Cursor<'_>) -> bool {
    let Some(idx) = cur.nth(0) else {
        return false;
    };
    match cur.tokens().kind(idx) {
        TokenKind::Dot
        | TokenKind::QuestionDot
        | TokenKind::Catch
        | TokenKind::CatchAll
        | TokenKind::CatchAllPanics
        | TokenKind::Is
        | TokenKind::Instanceof
        | TokenKind::Else => true,
        TokenKind::Minus => {
            let next = TokenIdx(idx.0 + 1);
            next.index() < cur.tokens().len() && cur.tokens().kind(next) == TokenKind::Whitespace
        }
        _ => BinaryOp::at(cur.tokens(), idx).is_some(),
    }
}

/// A top-level `let`/`const`/`let type` statement, through its `;` if it has
/// one. A statement does not end at a comma (`map<string, int>`), only at `;`
/// or a line break.
pub(crate) fn stub_top_level_stmt(p: &mut Parser<'_, '_>) -> Id<Bogus> {
    let mark = p.cur.mark();
    stub_run(&mut p.cur, false, &[TokenKind::Semicolon]);
    p.cur.eat::<punct::Semicolon>();
    let tokens = p.cur.range_from(mark);
    p.out.placeholder(tokens)
}

/// A `test`/`testset` item: its name and `with` clause are expressions and
/// its body a block.
pub(crate) fn stub_test_item(p: &mut Parser<'_, '_>) -> Id<Bogus> {
    let mark = p.cur.mark();
    p.cur.skip(); // test / testset
    stub_run(&mut p.cur, true, &[TokenKind::LBrace, TokenKind::With]);
    if p.cur.at(TokenKind::With) {
        p.cur.skip();
        stub_run(&mut p.cur, true, &[TokenKind::LBrace]);
    }
    p.cur.skip_balanced_braces(at_item_head);
    let tokens = p.cur.range_from(mark);
    p.out.placeholder(tokens)
}
