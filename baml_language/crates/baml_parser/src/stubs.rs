//! The one placeholder left before the template layer lands: a backtick
//! string is a real [`BacktickString`] node — its opener and closer owned, so
//! every position that holds one can be built — whose interior is a single
//! `Bogus` placeholder instead of parts. The parity oracle excludes the
//! interiors; the template layer replaces this module, and with it goes
//! `Sink::placeholder`.

#![expect(deprecated, reason = "the stub is built on the placeholder sink entry")]

use baml_syntax::{
    Token,
    nodes::{AnyTemplatePart, BacktickString},
    token::{TokenKind, lit},
    tree::Id,
};

use crate::{
    cursor::{Cursor, Mark},
    parser::{Parse, Parser},
};

/// After a backtick opener: the template as a node with a placeholder
/// interior.
impl Parse for BacktickString {
    type Head = Token![BacktickStart];
    fn parse(p: &mut Parser<'_, '_>, open: Self::Head) -> Id<Self> {
        let interior = p.cur.mark();
        skip_interior(&mut p.cur);
        let tokens = p.cur.range_from(interior);
        let parts = if tokens.is_empty() {
            Vec::new()
        } else {
            vec![AnyTemplatePart::Bogus(p.out.placeholder(tokens))]
        };
        let parts = p.out.alloc_list(parts);
        let close = p.expect::<lit::BacktickEnd>();
        p.alloc(Mark::of(open), BacktickString { open, parts, close })
    }
}

/// After an opener: consume up to (not including) the matching closer;
/// nested templates are opaque.
fn skip_interior(cur: &mut Cursor<'_>) {
    let mut depth = 0u32;
    while let Some(kind) = cur.kind(0) {
        match kind {
            TokenKind::BacktickStart => depth += 1,
            TokenKind::BacktickEnd => {
                if depth == 0 {
                    return;
                }
                depth -= 1;
            }
            _ => {}
        }
        cur.skip();
    }
}
