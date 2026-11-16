//! Control flow: `if` and `if let` chains, `match`, `catch`, lambdas and
//! `spawn`.
//!
//! An `if … { tail } else { tail }` in expression position may stay on one
//! line when it fits; every other `if` chain breaks every block, the chain
//! as a whole, as rustfmt lays an `if` out. A block's tail is statement
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
    nodes::{
        AnyElse, AnyExpr, AnyMatchScrutinee, Block, CatchArm, CatchClause, CatchExpr, ElseBranch,
        IfExpr, IfLetExpr, LambdaExpr, MatchArm, MatchExpr, MatchParen, SpawnExpr,
    },
    token::{Token, punct},
    tree::{AnyNodeId, Id, NodeUnion as _},
};

use super::{exprs::with_tail, items::signature_tail, stmts::parenthesised};
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
    /// An `if let` header is a named group, so that the `{` can take a line
    /// of its own when the scrutinee broke over several lines (an `if`'s
    /// condition has its parentheses for that).
    fn parts<'a>(
        self,
        f: &Formatter<'a>,
    ) -> (Doc<'a>, Option<GroupId<'a>>, Id<Block>, Option<ElseBranch>) {
        let d = f.docs;
        match self {
            Self::If(id) => {
                let node = f.tree.get(id);
                let header = d.concat([
                    f.token(node.kw),
                    Doc::SPACE,
                    parenthesised(f, f.tree.present(node.cond)),
                ]);
                (
                    header,
                    None,
                    f.tree.present(node.then_branch),
                    node.else_branch,
                )
            }
            Self::IfLet(id) => {
                let node = f.tree.get(id);
                let (header, group) = d.group_with_id(d.concat([
                    f.token(node.kw),
                    Doc::SPACE,
                    f.token(node.introducer),
                    Doc::SPACE,
                    f.alt(f.tree.present(node.pattern)),
                    Doc::SPACE,
                    f.token(f.tree.present(node.eq)),
                    Doc::SPACE,
                    f.alt(f.tree.present(node.scrutinee)),
                ]));
                (
                    header,
                    Some(group),
                    f.tree.present(node.then_branch),
                    node.else_branch,
                )
            }
        }
    }

    /// Whether the chain is `if … { tail } else { tail }` — one `else`, a
    /// block, both only a tail — the shape that may stay on one line.
    fn is_short(self, f: &Formatter<'_>) -> bool {
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
/// is short (see [`IfHead::is_short`]) and fits, else every block broken.
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
        let (header, header_group, then_branch, else_branch) = link.parts(f);
        parts.extend([
            header,
            f.brace_after(header_group),
            f.with_comments(then_branch, f.block_parts(then_branch, flat)),
        ]);
        next = None;
        if let Some(else_branch) = else_branch {
            parts.extend([Doc::SPACE, f.token(else_branch.kw), Doc::SPACE]);
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
    if flat { d.group(doc) } else { doc }
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
                f.token(node.l),
                d.indent(
                    d.concat([
                        Doc::SOFT_LINE,
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

/// `head => body,` — the comma is canonical. `head` is the pattern, and a
/// guard if there is one, as one group: while it is on one line the body
/// goes after the `=>` as an initialiser goes after a `=`; once the head
/// broke over several lines (a long guard, a union pattern) the body takes a
/// line of its own under it, indented, so that the pattern's lines and the
/// body's are told apart. A braced body (a block, an `if`, a `match`, …)
/// breaks inside its own braces instead and stays on the head's last line,
/// `=> {`, as rustfmt keeps it after a multi-line pattern.
fn arm<'a>(
    f: &Formatter<'a>,
    head: Doc<'a>,
    arrow: Token<punct::FatArrow>,
    body: AnyExpr,
    comma: Option<Token<punct::Comma>>,
) -> Doc<'a> {
    let d = f.docs;
    let (head, head_group) = d.group_with_id(head);
    let body_id = body.node_id();
    let body_doc = f.alt(body);
    let assigned = f.assigned_doc([arrow.idx], body_id, body_doc);
    let after_arrow = if braced(body_id) {
        assigned
    } else {
        d.if_group_breaks(
            head_group,
            d.indent(d.concat([Doc::HARD_LINE, body_doc])),
            assigned,
        )
    };
    d.concat([
        head,
        Doc::SPACE,
        f.token(arrow),
        after_arrow,
        f.separator(comma, ","),
    ])
}

impl FormatNode for MatchArm {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        let d = f.docs;
        let pattern = f.alt(f.tree.present(node.pattern));
        // A guard that does not fit beside the pattern goes under it.
        let guard = node.guard.map_or(Doc::NIL, |guard| {
            d.indent(d.concat([
                Doc::SOFT_LINE_OR_SPACE,
                f.token(guard.kw),
                Doc::SPACE,
                f.alt(f.tree.present(guard.cond)),
            ]))
        });
        arm(
            f,
            d.concat([pattern, guard]),
            f.tree.present(node.arrow),
            f.tree.present(node.body),
            node.comma,
        )
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
            f.token(node.kw),
            Doc::SPACE,
            f.token(f.tree.present(node.l)),
            f.node(f.tree.present(node.binding)),
            node.trace.map_or(Doc::NIL, |trace| {
                d.concat([
                    f.token(trace.comma),
                    Doc::SPACE,
                    f.node(f.tree.present(trace.name)),
                ])
            }),
            f.token(f.tree.present(node.r)),
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
        arm(
            f,
            f.alt(f.tree.present(node.pattern)),
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
/// printed as `->`. The signature breaks like a function's: parameters
/// first, then `throws` under the return type, the `{` on a line of its own
/// when that happened (see [`signature`](super::items::signature)).
pub(super) fn lambda<'a>(f: &Formatter<'a>, id: Id<LambdaExpr>) -> Forms<'a> {
    let node = f.tree.get(id);
    let d = f.docs;
    if let Some(generics) = node.rejected_generics {
        f.tree.absent(generics);
    }
    let arrow = d.indent(f.spelled("->", f.tree.present(node.arrow).idx));
    let ret = match node.ret {
        Some(ty) => d.concat([arrow, Doc::SPACE, f.alt(ty)]),
        None => arrow,
    };
    let (tail, tail_group) = signature_tail(f, ret, node.throws);
    let header = node.throws.map(|_| tail_group);
    let head = d.concat([
        super::items::signature(f, f.node(node.params), tail),
        f.brace_after(header),
    ]);
    let (flat, broken) = f.block_forms(f.tree.present(node.body));
    Forms {
        flat: d.concat([head, f.body_after(header, flat, broken)]),
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
/// its own (see [`Formatter::separated`]).
pub(super) fn spawn<'a>(f: &Formatter<'a>, id: Id<SpawnExpr>) -> Forms<'a> {
    let node = f.tree.get(id);
    let d = f.docs;
    let (with, with_group) = match node.with {
        Some(with) => {
            let (with, group) = super::items::with_clause(f, with);
            (d.concat([Doc::SPACE, with]), Some(group))
        }
        None => (Doc::NIL, None),
    };
    // A comment that ends the keyword's line puts the name one level in.
    let head = d.concat([
        f.token(node.kw),
        d.indent(
            node.name
                .map_or(Doc::NIL, |name| d.concat([Doc::SPACE, f.alt(name)])),
        ),
        with,
        f.brace_after(with_group),
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
