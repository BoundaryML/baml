//! Pure lookaheads over the significant tokens: each takes a [`Cursor`] by
//! value and reads ahead without consuming. They decide only what cannot be
//! decided after parsing without re-reading input — whether a `(` opens a
//! lambda, a `<` a generic argument list, a `{` a map — and nothing a later
//! consume has to trust beyond the token it is at.

use baml_syntax::{
    ops::BinaryOp,
    token::{Ident, KindSet, TokenKind, TokenSet as _, sets::StringToken},
};

use crate::{cursor::Cursor, expr::at_expression_start};

/// Reserved words accepted as later segments of a path (`baml.spawn.Plan`).
pub(crate) const PATH_SEGMENT: KindSet = Ident::KINDS.union(KindSet::new(&[
    TokenKind::Spawn,
    TokenKind::Await,
    TokenKind::Class,
    TokenKind::Enum,
    TokenKind::Interface,
    TokenKind::Function,
]));

const NUMBER: KindSet = KindSet::new(&[TokenKind::Bigint, TokenKind::Integer, TokenKind::Float]);

/// What may appear inside a `<…>` that is a generic argument list rather than
/// a comparison: names and paths, type punctuation, literal types, and the
/// punctuation of a function type.
const IN_TYPE_ARGS: KindSet =
    PATH_SEGMENT
        .union(StringToken::KINDS)
        .union(NUMBER)
        .union(KindSet::new(&[
            TokenKind::Dot,
            TokenKind::Comma,
            TokenKind::Equals,
            TokenKind::LBracket,
            TokenKind::RBracket,
            TokenKind::Question,
            TokenKind::Pipe,
            TokenKind::Minus,
            TokenKind::LParen,
            TokenKind::RParen,
            TokenKind::Colon,
            TokenKind::Arrow,
            TokenKind::FatArrow,
        ]));

/// After the `}` of a brace literal in a position where a `{` would otherwise
/// open the enclosing construct's body: tokens that continue the expression
/// (so the literal was one), or a second `{` (the body itself).
const CONTINUES_AFTER_BRACE: KindSet = KindSet::new(&[
    TokenKind::LBrace,
    TokenKind::Dot,
    TokenKind::Dollar,
    TokenKind::QuestionDot,
    TokenKind::LBracket,
    TokenKind::LParen,
]);

/// `let`, or a contextual `const` followed by a pattern-shaped token.
pub(crate) fn at_binding_intro(cur: Cursor<'_>) -> bool {
    binding_intro_at(cur, 0)
}

/// [`at_binding_intro`] at the significant token `n` ahead.
pub(crate) fn binding_intro_at(cur: Cursor<'_>, n: usize) -> bool {
    cur.nth_is(n, TokenKind::Let)
        || (cur.nth_is(n, TokenKind::Const)
            && (cur.nth_is_ident(n + 1) || cur.nth_is(n + 1, TokenKind::LBracket)))
}

pub(crate) fn at_catch_clause_start(cur: Cursor<'_>) -> bool {
    matches!(
        cur.kind(0),
        Some(TokenKind::Catch | TokenKind::CatchAll | TokenKind::CatchAllPanics)
    )
}

/// The significant index of the closer matching the opener at `open`, by a
/// stack of expected closers; `None` when the input ends or a closer is out
/// of order. A `${` opens a brace, a backtick run a template.
pub(crate) fn matching_close(cur: Cursor<'_>, open: usize) -> Option<usize> {
    let mut stack: Vec<TokenKind> = Vec::new();
    let mut i = open;
    loop {
        let kind = cur.kind_at_sig(i)?;
        match kind {
            TokenKind::LParen => stack.push(TokenKind::RParen),
            TokenKind::LBracket => stack.push(TokenKind::RBracket),
            TokenKind::LBrace | TokenKind::DollarLBrace => stack.push(TokenKind::RBrace),
            TokenKind::BacktickStart => stack.push(TokenKind::BacktickEnd),
            TokenKind::RParen
            | TokenKind::RBracket
            | TokenKind::RBrace
            | TokenKind::BacktickEnd => {
                if stack.pop() != Some(kind) {
                    return None;
                }
                if stack.is_empty() {
                    return Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
}

/// The significant index of the `>` closing the generic argument list that
/// opens at `open` (a `<`), if the tokens between can only be type
/// arguments. `>` is one character, so `>>` closes two lists — only when two
/// are open — and `>=`/`>>=` cannot close one at all (they are operators, so
/// the `<` was a comparison).
pub(crate) fn generic_args_close(cur: Cursor<'_>, open: usize) -> Option<usize> {
    debug_assert_eq!(cur.kind_at_sig(open), Some(TokenKind::Less));
    let mut depth = 1u32;
    let mut i = open + 1;
    loop {
        let kind = cur.kind_at_sig(i)?;
        match kind {
            // The removed `unreflect(expr)` occupies one argument slot; its
            // operand is an expression, skipped balanced.
            TokenKind::Unreflect if cur.kind_at_sig(i + 1) == Some(TokenKind::LParen) => {
                i = matching_close(cur, i + 1)? + 1;
                continue;
            }
            TokenKind::Less => depth += 1,
            TokenKind::Greater => {
                let idx = cur.sig(i)?;
                match BinaryOp::at(cur.tokens(), idx) {
                    Some(BinaryOp::Gt) => depth -= 1,
                    Some(BinaryOp::Shr) if depth >= 2 => {
                        depth -= 2;
                        i += 1;
                    }
                    _ => return None,
                }
                if depth == 0 {
                    return Some(i);
                }
            }
            kind if IN_TYPE_ARGS.contains(kind) => {}
            _ => return None,
        }
        i += 1;
    }
}

/// Whether the `<` at the cursor opens generic arguments rather than a
/// comparison: the list must close ([`generic_args_close`]) and what follows
/// the `>` must not read as the right operand of a comparison — the
/// TypeScript rule, so `foo<int>` is a value and `[a < b, c > d]` two
/// comparisons.
pub(crate) fn looks_like_generic_args(cur: Cursor<'_>) -> bool {
    let Some(close) = generic_args_close(cur, cur.pos()) else {
        return false;
    };
    let follow = close + 1;
    match cur.kind_at_sig(follow) {
        // A call, a generic constructor, a qualified path.
        Some(TokenKind::LParen | TokenKind::LBrace | TokenKind::Dot) => true,
        // Ambiguous with a comparison or a shift: the comparison reading wins.
        Some(
            TokenKind::Less
            | TokenKind::LessLess
            | TokenKind::Greater
            | TokenKind::Plus
            | TokenKind::Minus,
        ) => false,
        None => true,
        Some(kind) => {
            cur.newline_between_sig(close, follow)
                || cur
                    .sig(follow)
                    .is_some_and(|idx| BinaryOp::at(cur.tokens(), idx).is_some())
                || !at_expression_start(at_sig(cur, follow))
                || kind == TokenKind::Comma
        }
    }
}

/// A copy of `cur` positioned at significant index `i`.
fn at_sig(cur: Cursor<'_>, i: usize) -> Cursor<'_> {
    let mut at = cur;
    at.set_pos(i);
    at
}

/// `<T>(…` / `<T, U>(…`: a lambda with (rejected) generic parameters.
pub(crate) fn looks_like_generic_lambda(cur: Cursor<'_>) -> bool {
    if !cur.at(TokenKind::Less) || !cur.nth_is_ident(1) {
        return false;
    }
    match cur.kind(2) {
        Some(TokenKind::Greater) => cur.nth_is(3, TokenKind::LParen),
        Some(TokenKind::Comma) => {
            let mut i = cur.pos() + 3;
            loop {
                match cur.kind_at_sig(i) {
                    Some(TokenKind::Greater) => {
                        return cur.kind_at_sig(i + 1) == Some(TokenKind::LParen);
                    }
                    Some(_) => i += 1,
                    None => return false,
                }
            }
        }
        _ => false,
    }
}

/// Whether the `(` at the cursor opens a lambda's parameters: `() ->`, a
/// typed first parameter `(name:`, or a balanced group followed by an arrow.
pub(crate) fn looks_like_lambda(cur: Cursor<'_>) -> bool {
    debug_assert!(cur.at(TokenKind::LParen));
    if cur.nth_is_ident(1) && cur.nth_is(2, TokenKind::Colon) {
        return true;
    }
    matching_close(cur, cur.pos()).is_some_and(|close| {
        matches!(
            cur.kind_at_sig(close + 1),
            Some(TokenKind::Arrow | TokenKind::FatArrow)
        )
    })
}

/// Whether the `(` at the cursor is followed, after its `)`, by `->`: a
/// function type in pattern position.
pub(crate) fn looks_like_function_type_paren(cur: Cursor<'_>) -> bool {
    debug_assert!(cur.at(TokenKind::LParen));
    matching_close(cur, cur.pos())
        .is_some_and(|close| cur.kind_at_sig(close + 1) == Some(TokenKind::Arrow))
}

/// Whether the `(` at the cursor is followed, after its `)`, by a type
/// modifier (`[` or `?`): a parenthesized type such as `(int | string)[]`.
pub(crate) fn looks_like_paren_type_suffix(cur: Cursor<'_>) -> bool {
    debug_assert!(cur.at(TokenKind::LParen));
    matching_close(cur, cur.pos()).is_some_and(|close| {
        matches!(
            cur.kind_at_sig(close + 1),
            Some(TokenKind::LBracket | TokenKind::Question)
        )
    })
}

/// Tokens that may end the `Self` type of a qualified projection, so that an
/// `as` after one of them is the projection's and not a name.
const ENDS_PROJECTION_SELF: KindSet = PATH_SEGMENT.union(KindSet::new(&[
    TokenKind::Greater,
    TokenKind::RBracket,
    TokenKind::Question,
    TokenKind::RParen,
]));

/// `( … as … ) . member` at the cursor: the qualified-projection shape, the
/// same in type and expression position.
pub(crate) fn looks_like_qualified_projection(cur: Cursor<'_>) -> bool {
    if !cur.at(TokenKind::LParen) {
        return false;
    }
    let mut i = cur.pos() + 1;
    let mut paren_depth = 1u32;
    let mut angle_depth = 0u32;
    let mut saw_as = false;
    let mut previous: Option<TokenKind> = None;
    let close = loop {
        let Some(kind) = cur.kind_at_sig(i) else {
            return false;
        };
        match kind {
            TokenKind::LParen => paren_depth += 1,
            TokenKind::RParen => {
                if paren_depth == 1 {
                    if angle_depth != 0 {
                        return false;
                    }
                    break i;
                }
                paren_depth -= 1;
            }
            TokenKind::Less => angle_depth += 1,
            TokenKind::Greater => angle_depth = angle_depth.saturating_sub(1),
            TokenKind::As
                if paren_depth == 1
                    && angle_depth == 0
                    && previous.is_some_and(|k| ENDS_PROJECTION_SELF.contains(k)) =>
            {
                saw_as = true;
            }
            _ => {}
        }
        previous = Some(kind);
        i += 1;
    };
    saw_as
        && cur.kind_at_sig(close + 1) == Some(TokenKind::Dot)
        && cur
            .kind_at_sig(close + 2)
            .is_some_and(|k| crate::types::MEMBER_NAME.contains(k))
}

/// `. as <` at the cursor: the `.as<T>` upcast.
pub(crate) fn looks_like_as_projection(cur: Cursor<'_>) -> bool {
    cur.at(TokenKind::Dot) && cur.nth_is(1, TokenKind::As) && cur.nth_is(2, TokenKind::Less)
}

/// Whether the `{` at the cursor opens a map literal rather than a block: a
/// top-level `:` before any `,`/`;`/statement keyword says map; `{}` is an
/// empty map; a `//#` header right inside says block.
pub(crate) fn looks_like_map(cur: Cursor<'_>) -> bool {
    debug_assert!(cur.at(TokenKind::LBrace));
    {
        let mut inside = cur;
        inside.skip();
        if inside.header_ahead() {
            return false;
        }
    }
    if cur.nth_is(1, TokenKind::RBrace) {
        return true;
    }
    let mut stack: Vec<TokenKind> = Vec::new();
    let mut i = cur.pos() + 1;
    while let Some(kind) = cur.kind_at_sig(i) {
        if kind == TokenKind::Less
            && stack.is_empty()
            && let Some(close) = generic_args_close(cur, i)
        {
            i = close + 1;
            continue;
        }
        match kind {
            TokenKind::Colon if stack.is_empty() => return true,
            TokenKind::Comma | TokenKind::Semicolon if stack.is_empty() => return false,
            TokenKind::Let | TokenKind::While | TokenKind::For if stack.is_empty() => {
                return false;
            }
            TokenKind::LParen => stack.push(TokenKind::RParen),
            TokenKind::LBracket => stack.push(TokenKind::RBracket),
            TokenKind::LBrace | TokenKind::DollarLBrace => stack.push(TokenKind::RBrace),
            TokenKind::BacktickStart => stack.push(TokenKind::BacktickEnd),
            TokenKind::RParen
            | TokenKind::RBracket
            | TokenKind::RBrace
            | TokenKind::BacktickEnd
                if stack.pop() != Some(kind) =>
            {
                return false;
            }
            _ => {}
        }
        i += 1;
    }
    false
}

/// At a name: the significant index of the `{` that makes this a class
/// destructure (`Class {`, `a.b.Class {`, `Class<int> {`), if there is one.
pub(crate) fn destructure_brace(cur: Cursor<'_>) -> Option<usize> {
    debug_assert!(cur.at_ident());
    let mut i = cur.pos() + 1;
    loop {
        match cur.kind_at_sig(i) {
            Some(TokenKind::Dot)
                if cur
                    .kind_at_sig(i + 1)
                    .is_some_and(|k| PATH_SEGMENT.contains(k)) =>
            {
                i += 2;
            }
            Some(TokenKind::LBrace) => return Some(i),
            Some(TokenKind::Less) => {
                let close = generic_args_close(cur, i)?;
                return (cur.kind_at_sig(close + 1) == Some(TokenKind::LBrace))
                    .then_some(close + 1);
            }
            _ => return None,
        }
    }
}

/// Whether the `{` at significant index `open` opens something field-shaped:
/// `{ name:`, a shorthand `{ name,` / `{ name }`, `{ ...`, or `{}`.
pub(crate) fn brace_looks_like_fields(cur: Cursor<'_>, open: usize) -> bool {
    debug_assert_eq!(cur.kind_at_sig(open), Some(TokenKind::LBrace));
    match cur.kind_at_sig(open + 1) {
        Some(TokenKind::DotDotDot | TokenKind::RBrace) => true,
        Some(kind) if Ident::KINDS.contains(kind) || kind == TokenKind::Client => {
            let mut i = open + 2;
            while cur.kind_at_sig(i) == Some(TokenKind::Dot) && cur.nth_is_ident_at(i + 1) {
                i += 2;
            }
            match cur.kind_at_sig(i) {
                Some(TokenKind::Colon) => true,
                Some(TokenKind::Comma | TokenKind::RBrace) => i == open + 2,
                _ => false,
            }
        }
        _ => false,
    }
}

/// Whether the brace literal opening at significant index `open` may be
/// taken where a `{` would otherwise be the enclosing construct's body: only
/// when the token after its `}` continues the expression, or is the body's
/// own `{`.
pub(crate) fn brace_literal_before_body(cur: Cursor<'_>, open: usize) -> bool {
    matching_close(cur, open).is_some_and(|close| {
        cur.kind_at_sig(close + 1)
            .is_some_and(|k| CONTINUES_AFTER_BRACE.contains(k))
    })
}

/// At a binding introducer: whether the header is the `in` form. The
/// pattern is skipped balanced; the first top-level `in` says so, a `=` or
/// `;` says C-style.
pub(crate) fn looks_like_for_in(cur: Cursor<'_>) -> bool {
    debug_assert!(at_binding_intro(cur));
    let mut stack: Vec<TokenKind> = Vec::new();
    let mut i = cur.pos() + 1;
    loop {
        let Some(kind) = cur.kind_at_sig(i) else {
            return false;
        };
        match kind {
            TokenKind::LParen => stack.push(TokenKind::RParen),
            TokenKind::LBracket => stack.push(TokenKind::RBracket),
            TokenKind::LBrace => stack.push(TokenKind::RBrace),
            TokenKind::RParen | TokenKind::RBracket | TokenKind::RBrace => {
                if stack.pop() != Some(kind) {
                    return false;
                }
            }
            TokenKind::In if stack.is_empty() => return true,
            TokenKind::Equals | TokenKind::Semicolon if stack.is_empty() => return false,
            _ => {}
        }
        i += 1;
    }
}

/// After a catch arm lost its `=>`: whether another arm's `=>` is ahead on
/// this line run, before a `}`, `,` or `;` (bounded, so recovery never scans
/// a whole file).
pub(crate) fn looks_like_catch_arm_ahead(cur: Cursor<'_>) -> bool {
    (cur.pos()..cur.pos() + 64)
        .map_while(|i| cur.kind_at_sig(i))
        .find(|kind| {
            matches!(
                kind,
                TokenKind::RBrace | TokenKind::Comma | TokenKind::Semicolon | TokenKind::FatArrow
            )
        })
        == Some(TokenKind::FatArrow)
}
