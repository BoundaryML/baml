//! Control flow: `if` and `if let` chains, `match`, `catch`, lambdas and
//! `spawn`.
//!
//! An `if … { tail } else { tail }` in expression position may stay on one
//! line when it fits within the width rustfmt allows a one-line `if`; every
//! other `if` chain breaks every block, the chain as a whole, as rustfmt
//! lays an `if` out. A block's tail is statement
//! position, as it is for rustfmt: an `if` that is a function's value
//! breaks. A `match` always breaks, one arm
//! per line with a trailing comma. An arm's body goes after the `=>` as a
//! `let`'s initialiser goes after its `=` ([`Formatter::assigned`]); a
//! braceless `return`, `break`, `continue` or `throw` body stays braceless,
//! and a block body stays a block. `catch` clauses follow the expression
//! they guard, their arms laid out as a `match`'s. A lambda's signature
//! breaks like a function's; its body, like a `spawn`'s, may stay on one
//! line when it is only a tail.

use baml_syntax::{
    Token,
    nodes::{
        AnyElse, AnyExpr, AnyMatchScrutinee, AnyPattern, Block, CatchArm, CatchClause, CatchExpr,
        ElseBranch, IfExpr, IfLetExpr, LambdaExpr, MatchArm, MatchExpr, MatchParen, SpawnExpr,
    },
    tree::{AnyNodeId, Id, NodeUnion as _},
};

use super::{exprs::with_tail, patterns::union_pattern, stmts::parenthesised};
use crate::{
    comments::docs,
    format::{FormatNode, Formatter, layout::braced},
    ir::{Doc, GroupId},
};

// ---------------------------------------------------------------------------
// `if` chains
// ---------------------------------------------------------------------------

/// The head of an `if` chain, or a link of it after an `else`.
#[derive(Debug, Clone, Copy)]
pub(super) enum IfHead {
    If(Id<IfExpr>),
    IfLet(Id<IfLetExpr>),
}

impl IfHead {
    fn node_id(self) -> AnyNodeId {
        match self {
            Self::If(id) => AnyNodeId::of(id),
            Self::IfLet(id) => AnyNodeId::of(id),
        }
    }

    /// The header up to the `{`, the block after it, and the `else` if any.
    /// The `{` follows the header after a [`Doc::HANG_LINE`]: on its last
    /// line when that is the header's own (`) {`, `} {`), on a line of its
    /// own when the scrutinee's last line is a continuation line. In an `if
    /// let` header, a comment that ends the pattern's line puts the `=` and
    /// the scrutinee one level in.
    fn parts<'a>(self, f: &Formatter<'a>) -> (Doc<'a>, Id<Block>, Option<ElseBranch>) {
        let d = f.docs;
        match self {
            Self::If(id) => {
                let node = f.tree.get(id);
                let header = d.concat([
                    f.token(node.kw),
                    Doc::SPACE,
                    parenthesised(f, f.tree.present(node.cond)),
                ]);
                (header, f.tree.present(node.then_branch), node.else_branch)
            }
            Self::IfLet(id) => {
                let node = f.tree.get(id);
                let pattern = f.tree.present(node.pattern);
                let header = d.group(d.concat([
                    f.token(node.kw),
                    Doc::SPACE,
                    f.token(node.introducer),
                    Doc::SPACE,
                    f.alt(pattern),
                    f.continuation(
                        pattern.node_id(),
                        d.concat([
                            Doc::SPACE,
                            f.token(f.tree.present(node.eq)),
                            Doc::SPACE,
                            f.alt(f.tree.present(node.scrutinee)),
                        ]),
                    ),
                ]));
                (header, f.tree.present(node.then_branch), node.else_branch)
            }
        }
    }

    /// Whether the chain is `if … { tail } else { tail }` — one `else`, a
    /// block, both only a tail, no comment anywhere in it — the shape that
    /// may stay on one line.
    fn is_short(self, f: &Formatter<'_>) -> bool {
        if f.has_comments_inside(self.node_id()) {
            return false;
        }
        let (then_branch, else_branch) = match self {
            Self::If(id) => {
                let node = f.tree.get(id);
                (f.tree.present(node.then_branch), node.else_branch)
            }
            Self::IfLet(id) => {
                let node = f.tree.get(id);
                (f.tree.present(node.then_branch), node.else_branch)
            }
        };
        let Some(else_branch) = else_branch else {
            return false;
        };
        let AnyElse::Block(else_block) = f.tree.present(else_branch.body) else {
            return false;
        };
        f.tail_only(then_branch) && f.tail_only(else_block)
    }
}

/// The chain from `head`, laid out as one: with `flat`, on one line when it
/// is short (see [`IfHead::is_short`]) and fits within the width for a
/// one-line `if` (see [`Heuristics`](crate::format::Heuristics)), else
/// every block broken.
/// The `if`s after the first are not printed as nodes of their own, so
/// their leading and trailing comments are placed here.
pub(super) fn if_chain<'a>(f: &Formatter<'a>, head: IfHead, flat: bool) -> Doc<'a> {
    let d = f.docs;
    let flat = flat && head.is_short(f);
    let mut parts = Vec::new();
    let mut trailing = Vec::new();
    let mut next = Some(head);
    while let Some(link) = next {
        if link.node_id() != head.node_id() {
            parts.push(docs::leading(d, f.tree, f.comments.leading(link.node_id())));
            trailing.push(docs::trailing(
                d,
                f.tree,
                f.comments.trailing(link.node_id()),
            ));
        }
        let (header, then_branch, else_branch) = link.parts(f);
        parts.extend([
            header,
            Doc::HANG_LINE,
            f.with_comments(then_branch, f.block_parts(then_branch, flat)),
        ]);
        next = None;
        if let Some(else_branch) = else_branch {
            parts.extend([Doc::SPACE, f.keyword(else_branch.kw), Doc::SPACE]);
            match f.tree.present(else_branch.body) {
                AnyElse::Block(block) => {
                    parts.push(f.with_comments(block, f.block_parts(block, flat)));
                }
                AnyElse::If(id) => next = Some(IfHead::If(id)),
                AnyElse::IfLet(id) => next = Some(IfHead::IfLet(id)),
                AnyElse::Bogus(bogus) => f.tree.absent(bogus),
            }
        }
    }
    parts.extend(trailing.into_iter().rev());
    let doc = d.concat(parts);
    if flat {
        d.group(d.limited(doc, f.heuristics.if_else))
    } else {
        doc
    }
}

/// Whether an expression never yields a value: `return`, `throw`, `break`,
/// `continue`. As a block's tail it is closed with a `;`, as rustfmt closes
/// one — the block is `never` either way.
pub(crate) fn diverges(expr: AnyExpr) -> bool {
    matches!(
        expr,
        AnyExpr::Return(_) | AnyExpr::Throw(_) | AnyExpr::Break(_) | AnyExpr::Continue(_)
    )
}

/// An expression in statement position — a statement's, or a block's tail:
/// a block, an `if` chain or a `spawn` is always broken there, however
/// short, as a `defer` is.
pub(in crate::format) fn statement<'a>(f: &Formatter<'a>, expr: AnyExpr) -> Doc<'a> {
    match expr {
        AnyExpr::Block(block) => f.expanded_block(block),
        AnyExpr::If(id) => f.with_comments(id, if_chain(f, IfHead::If(id), false)),
        AnyExpr::IfLet(id) => f.with_comments(id, if_chain(f, IfHead::IfLet(id), false)),
        AnyExpr::Spawn(id) => f.with_comments(id, spawn(f, id).hugged),
        other => f.alt(other),
    }
}

/// An `if` in expression position: on one line when short and it fits.
impl FormatNode for IfExpr {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        if_chain(f, IfHead::If(id), true)
    }
}

impl FormatNode for IfLetExpr {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        if_chain(f, IfHead::IfLet(id), true)
    }
}

// ---------------------------------------------------------------------------
// `match` and `catch`
// ---------------------------------------------------------------------------

/// `match (scrutinee) { arms }`, the parentheses added when the source left
/// them out — a tuple scrutinee brings its own — one arm per line.
impl FormatNode for MatchExpr {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        let scrutinee = match f.tree.present(node.scrutinee) {
            AnyMatchScrutinee::Paren(paren) => f.node(paren),
            // A tuple scrutinee's own parentheses are the match's: `match (a, b)`.
            AnyMatchScrutinee::Bare(tuple @ AnyExpr::Tuple(_)) => f.alt(tuple),
            AnyMatchScrutinee::Bare(expr) => parenthesised(f, expr),
            AnyMatchScrutinee::Bogus(bogus) => f.tree.absent(bogus),
        };
        let arms: Vec<AnyNodeId> = f
            .tree
            .list(node.arms)
            .iter()
            .map(|&arm| AnyNodeId::of(arm))
            .collect();
        f.docs.concat([
            f.token(node.kw),
            Doc::SPACE,
            scrutinee,
            Doc::SPACE,
            f.braced(
                AnyNodeId::of(id),
                f.tree.present(node.l),
                &arms,
                f.tree.present(node.r),
                false,
            ),
        ])
    }
}

/// `(scrutinee: T)`, the inside on a line of its own when it does not fit.
impl FormatNode for MatchParen {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        let d = f.docs;
        d.group(
            d.concat([
                Doc::LINE_SUFFIX_BOUNDARY,
                d.text("("),
                f.comments_inside(node.l.idx),
                d.indent(
                    d.concat([
                        f.line_after_opener(node.l.idx),
                        f.alt(f.tree.present(node.expr)),
                        node.ascription
                            .map_or(Doc::NIL, |ascription| f.node(ascription)),
                    ]),
                ),
                Doc::SOFT_LINE,
                f.token(f.tree.present(node.r)),
            ]),
        )
    }
}

/// `head => body,` — the comma is canonical. The body goes after the `=>`
/// as an initialiser goes after a `=`, whether or not the pattern broke over
/// several lines (a union's alternatives, a destructure's fields): the
/// `=>` is on the pattern's last line either way, as rustfmt keeps it.
/// Only a guard that broke under its pattern (`guard`, the named group)
/// sends a non-braced body to a line of its own under the head, indented,
/// so that the guard's lines and the body's are told apart. A braced body
/// (a block, an `if`, a `match`, …) breaks inside its own braces instead
/// and stays on the head's last line, `=> {`.
fn arm<'a>(
    f: &Formatter<'a>,
    head: Doc<'a>,
    head_ends_line: bool,
    guard: Option<GroupId<'a>>,
    arrow: Token!["=>"],
    body: AnyExpr,
    comma: Option<Token![","]>,
) -> Doc<'a> {
    let d = f.docs;
    let body_id = body.node_id();
    let body_doc = f.alt(body);
    let assigned = f.assigned_doc([arrow.idx], body_id, body_doc);
    let after_arrow = match guard {
        Some(guard) if !braced(body_id) => d.if_group_breaks(
            guard,
            d.indent(d.concat([Doc::HARD_LINE, body_doc])),
            assigned,
        ),
        Some(_) | None => assigned,
    };
    // After a comment that ends the head's line, the `=>` starts the next
    // one level in.
    let rest = d.concat([Doc::SPACE, f.token(arrow), after_arrow]);
    let rest = if head_ends_line { d.indent(rest) } else { rest };
    d.concat([head, rest, f.separator(comma, ",")])
}

impl FormatNode for MatchArm {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        let d = f.docs;
        let pattern_id = f.tree.present(node.pattern).node_id();
        let pattern = arm_pattern(f, f.tree.present(node.pattern));
        // A guard stays beside the pattern while its `if cond` starts there
        // (the condition breaking inside itself, as rustfmt keeps `1 if a`);
        // only when even that does not fit does it go under the pattern.
        // The condition is the named group: a body that is not braced
        // moves to a line of its own under the head once it broke.
        let (guard, guard_group, guard_ends_line) = match node.guard {
            Some(guard) => {
                let cond = f.tree.present(guard.cond);
                let (cond_doc, group) = d.group_with_id(f.alt(cond));
                let clause = d.concat([f.token(guard.kw), Doc::SPACE, cond_doc]);
                // The beside-the-pattern variant is measured up to the
                // condition's first line break, the condition laying itself
                // out.
                let doc = d.layouts([
                    d.concat([Doc::SPACE, clause]).broken(),
                    d.indent(d.concat([Doc::HARD_LINE, clause])).into(),
                ]);
                (doc, Some(group), f.trailing_of(cond.node_id()).will_break())
            }
            None => (Doc::NIL, None, false),
        };
        arm(
            f,
            d.concat([pattern, guard]),
            f.trailing_of(pattern_id).will_break() || guard_ends_line,
            guard_group,
            f.tree.present(node.arrow),
            f.tree.present(node.body),
            node.comma,
        )
    }
}

/// An arm's whole pattern: a union's alternatives break at the arm's
/// indent, as rustfmt lays them out.
fn arm_pattern<'a>(f: &Formatter<'a>, pattern: AnyPattern) -> Doc<'a> {
    match pattern {
        AnyPattern::Union(union) => f.with_comments(union, union_pattern(f, union, false)),
        other => f.alt(other),
    }
}

/// `body catch (e) { arms } catch (e, trace) { arms }`: the clauses follow
/// the body on its last line — inside a member chain's indentation when the
/// chain broke, so that they hang off its last call.
impl FormatNode for CatchExpr {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        let d = f.docs;
        let clauses = d.concat(
            f.tree
                .list(node.clauses.as_list())
                .iter()
                .map(|&clause| d.concat([Doc::SPACE, f.node(clause)])),
        );
        with_tail(f, node.body, clauses)
    }
}

impl FormatNode for CatchClause {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        let d = f.docs;
        let arms: Vec<AnyNodeId> = f
            .tree
            .list(node.arms)
            .iter()
            .map(|&arm| AnyNodeId::of(arm))
            .collect();
        d.concat([
            f.keyword(node.kw),
            Doc::SPACE,
            Doc::LINE_SUFFIX_BOUNDARY,
            d.text("("),
            f.comments_inside(f.tree.present(node.l).idx),
            f.space_after_opener(f.tree.present(node.l).idx),
            f.node(f.tree.present(node.binding)),
            node.trace.map_or(Doc::NIL, |trace| {
                d.concat([
                    f.token(trace.comma),
                    Doc::SPACE,
                    f.node(f.tree.present(trace.name)),
                ])
            }),
            // After a line comment on the `)`'s line the `{` lines up with
            // its `}`, as after `else`.
            f.keyword(f.tree.present(node.r)),
            Doc::SPACE,
            f.braced(
                AnyNodeId::of(id),
                f.tree.present(node.lbrace),
                &arms,
                f.tree.present(node.rbrace),
                false,
            ),
        ])
    }
}

impl FormatNode for CatchArm {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        let pattern = f.tree.present(node.pattern);
        arm(
            f,
            arm_pattern(f, pattern),
            f.trailing_of(pattern.node_id()).will_break(),
            None,
            f.tree.present(node.arrow),
            f.tree.present(node.body),
            node.comma,
        )
    }
}

// ---------------------------------------------------------------------------
// Lambdas and `spawn`
// ---------------------------------------------------------------------------

/// A lambda or `spawn` in both of its layouts, built from one printing of
/// its parts: `flat`, the body on one line when it is only a tail; `hugged`,
/// the body broken — as the last argument of a call, hugging the
/// parentheses, or in statement position.
pub(super) struct Forms<'a> {
    pub flat: Doc<'a>,
    pub hugged: Doc<'a>,
}

/// `(params) -> R throws E { body }`; a `=>` written for the arrow is
/// printed as `->`. The signature breaks like a function's: `throws` under
/// the return type first, then the parameters, the `{` on a line of its own
/// when the signature's last line is a continuation line (see
/// [`signature`](super::items::signature)); the body breaks once the
/// signature did.
pub(super) fn lambda<'a>(f: &Formatter<'a>, id: Id<LambdaExpr>) -> Forms<'a> {
    let node = f.tree.get(id);
    let d = f.docs;
    if let Some(generics) = node.rejected_generics {
        f.tree.absent(generics);
    }
    let arrow = f.spelled("->", f.tree.present(node.arrow).idx);
    let ret = match node.ret {
        Some(ty) => d.concat([arrow, Doc::SPACE, f.alt(ty)]),
        None => arrow,
    };
    let params = AnyNodeId::of(node.params);
    let (signature, group) = super::items::signature(
        f,
        d.concat([f.leading_of(params), f.bare(params)]),
        f.trailing_of(params),
        ret,
        Doc::NIL,
        node.throws,
    );
    let head = d.concat([signature, Doc::HANG_LINE]);
    let (flat, broken) = f.block_forms(f.tree.present(node.body));
    Forms {
        flat: d.concat([head, f.body_after(Some(group), flat, broken)]),
        hugged: d.concat([head, broken]),
    }
}

impl FormatNode for LambdaExpr {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        lambda(f, id).flat
    }
}

/// `spawn name with a, b { body }`: the `with` items break one per line
/// under the keyword when they do not fit, and the `{` then takes a line of
/// its own (see [`Formatter::separated`] and [`Doc::HANG_LINE`]).
pub(super) fn spawn<'a>(f: &Formatter<'a>, id: Id<SpawnExpr>) -> Forms<'a> {
    let node = f.tree.get(id);
    let d = f.docs;
    let (with, with_group) = match node.with {
        Some(with) => {
            let (clause, group) = super::items::with_clause(f, with);
            let clause = d.concat([Doc::SPACE, clause]);
            // After a comment that ends the name's line, `with` starts the
            // next one level in.
            let clause = match node.name {
                Some(name) => f.continuation(name.node_id(), clause),
                None => clause,
            };
            (clause, Some(group))
        }
        None => (Doc::NIL, None),
    };
    // A comment that ends the keyword's line puts the name one level in; a
    // bare `spawn` has only its block after it, which stays flush.
    let kw = if node.name.is_none() && node.with.is_none() {
        f.keyword(node.kw)
    } else {
        f.token(node.kw)
    };
    let head = d.concat([
        kw,
        d.indent(
            node.name
                .map_or(Doc::NIL, |name| d.concat([Doc::SPACE, f.alt(name)])),
        ),
        with,
        Doc::HANG_LINE,
    ]);
    let (flat, broken) = f.block_forms(f.tree.present(node.body));
    Forms {
        flat: d.concat([head, f.body_after(with_group, flat, broken)]),
        hugged: d.concat([head, broken]),
    }
}

impl FormatNode for SpawnExpr {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        spawn(f, id).flat
    }
}
