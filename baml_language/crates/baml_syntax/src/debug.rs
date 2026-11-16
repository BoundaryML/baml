//! An indented dump of a tree, for snapshots and debugging.
//!
//! ```text
//! CallExpr@8..14
//!   callee: PathExpr@8..9
//!     name: Name@8..9
//!       token: Word "f" @8..9
//!   args: ArgList@9..14
//!     l: LParen "(" @9..10
//!     args: Arg@10..11
//!       …
//!     r: <missing @14>
//! ```
//!
//! Ranges are byte offsets; a node's range is its token span, so it never
//! includes leading or trailing trivia.

use std::fmt::Write as _;

use text_size::TextRange;

use crate::{
    token::{TokenIdx, Tokens},
    tree::{AnyNodeId, Missing, TokenRange, TreeWalk, Visitor},
};

enum Part {
    Node(&'static str, AnyNodeId),
    Token(&'static str, TokenIdx),
    Missing(&'static str, Missing),
}

#[derive(Default)]
struct Collect(Vec<Part>);

impl Visitor for Collect {
    fn node(&mut self, field: &'static str, id: AnyNodeId) {
        self.0.push(Part::Node(field, id));
    }
    fn token(&mut self, field: &'static str, token: TokenIdx) {
        self.0.push(Part::Token(field, token));
    }
    fn range(&mut self, field: &'static str, range: TokenRange) {
        // Spelled out token by token, trivia included: the dump shows what
        // the node covers.
        self.0
            .extend(range.iter().map(|token| Part::Token(field, token)));
    }
    fn missing(&mut self, field: &'static str, missing: Missing) {
        self.0.push(Part::Missing(field, missing));
    }
}

pub fn debug_tree<A: TreeWalk>(arenas: &A, tokens: &Tokens, text: &str, root: AnyNodeId) -> String {
    let mut out = String::new();
    print_node(arenas, tokens, text, root, None, 0, &mut out);
    out
}

fn byte_range(tokens: &Tokens, arenas: &impl TreeWalk, id: AnyNodeId) -> Option<TextRange> {
    let span = arenas.span(id);
    if span.is_empty() {
        return None;
    }
    Some(TextRange::new(
        tokens.range(span.start()).start(),
        tokens.range(TokenIdx(span.end().0 - 1)).end(),
    ))
}

fn print_node<A: TreeWalk>(
    arenas: &A,
    tokens: &Tokens,
    text: &str,
    id: AnyNodeId,
    field: Option<&'static str>,
    depth: usize,
    out: &mut String,
) {
    let never = |_| unreachable!("String never fails");
    for _ in 0..depth {
        out.push_str("  ");
    }
    if let Some(field) = field {
        write!(out, "{field}: ").unwrap_or_else(never);
    }
    out.push_str(arenas.kind_name(id));
    match byte_range(tokens, arenas, id) {
        Some(range) => writeln!(
            out,
            "@{}..{}",
            u32::from(range.start()),
            u32::from(range.end())
        ),
        None => writeln!(out, "@<empty>"),
    }
    .unwrap_or_else(never);

    let mut parts = Collect::default();
    arenas.walk_node(id, &mut parts);
    for part in parts.0 {
        match part {
            Part::Node(field, child) => {
                print_node(arenas, tokens, text, child, Some(field), depth + 1, out);
            }
            Part::Token(field, token) => {
                for _ in 0..=depth {
                    out.push_str("  ");
                }
                let range = tokens.range(token);
                writeln!(
                    out,
                    "{field}: {:?} {:?} @{}..{}",
                    tokens.kind(token),
                    tokens.text(text, token),
                    u32::from(range.start()),
                    u32::from(range.end())
                )
                .unwrap_or_else(never);
            }
            Part::Missing(field, missing) => {
                for _ in 0..=depth {
                    out.push_str("  ");
                }
                writeln!(out, "{field}: <missing @{}>", u32::from(missing.at()))
                    .unwrap_or_else(never);
            }
        }
    }
}
