//! Statements and blocks: one statement per line, every statement closed by
//! `;` — except one that ends in a block, which keeps the `;` it was
//! written with, since as the last statement a `;` is what makes it a
//! statement rather than the block's tail — a stray `;` printing nothing, a
//! block's tail bare. Loop conditions and `for` headers are parenthesised. A
//! block or `if` chain in statement position is always broken; a block that
//! is only a tail may stay on one line elsewhere. What follows a `let`'s or
//! an assignment's `=` is laid out by [`Formatter::assigned`].

use baml_syntax::{
    nodes::{
        AnyExpr, AnyForHeader, AnyForInit, AnyForStep, AnyStmt, AssignStmt, Block, CForHeader,
        DeferStmt, EmptyStmt, ExprStmt, ForInHeader, ForStmt, HeaderStmt, LetElse, LetStmt,
        ParenForInHeader, TypeBindingStmt, WhileLetStmt, WhileStmt,
    },
    token::TokenIdx,
    tree::{Id, NodeUnion as _},
};

use super::control::statement;
use crate::{
    format::{FormatNode, Formatter, layout::braced},
    ir::Doc,
};

/// A block in expression position: `{ x }` when it is only a tail and fits.
/// One holding nothing but stray `;`s keeps one of them: `{}` there would
/// read as an empty map.
impl FormatNode for Block {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        let stmts = f.tree.list(node.stmts);
        if node.tail.is_none()
            && !stmts.is_empty()
            && stmts.iter().all(|stmt| matches!(stmt, AnyStmt::Empty(_)))
        {
            let d = f.docs;
            let comments = d.concat(stmts.iter().map(|&stmt| f.alt(stmt)));
            return d.concat([
                Doc::LINE_SUFFIX_BOUNDARY,
                d.text("{"),
                f.comments_after_opener(node.l.idx),
                d.text(" ;"),
                comments,
                Doc::SPACE,
                f.token(f.tree.present(node.r)),
            ]);
        }
        f.block(id, f.tail_only(id))
    }
}

/// An expression statement ending in braces of its own (see `braced`)
/// keeps the `;` it was written with rather than getting one added.
impl FormatNode for ExprStmt {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        let semi = if braced(node.expr.node_id()) {
            node.semi.map_or(Doc::NIL, |semi| f.token(semi))
        } else {
            f.separator(node.semi, ";")
        };
        f.docs.concat([statement(f, node.expr), semi])
    }
}

/// `target op value;`; `semi` is the `;` or, in a `for` step, nothing. After
/// a comment that ends the target's line, the operator starts the next one
/// level in.
fn assign<'a>(f: &Formatter<'a>, id: Id<AssignStmt>, semi: Doc<'a>) -> Doc<'a> {
    let node = f.tree.get(id);
    let d = f.docs;
    let mut tokens = node.op.tokens();
    let first = tokens
        .next()
        .unwrap_or_else(|| unreachable!("an operator has a token"));
    d.concat([
        f.alt(node.target),
        f.continuation(
            node.target.node_id(),
            d.concat([
                Doc::SPACE,
                f.spelled(node.op.op().spelling(), first),
                d.concat(tokens.map(|token| f.comments_after(token))),
                f.assigned(node.op.tokens(), f.tree.present(node.value)),
            ]),
        ),
        semi,
    ])
}

impl FormatNode for AssignStmt {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let semi = f.separator(f.tree.get(id).semi, ";");
        assign(f, id, semi)
    }
}

/// A stray `;` prints nothing; its comments stay.
impl FormatNode for EmptyStmt {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        f.comments_after(f.tree.get(id).semi.idx)
    }
}

/// The initialiser after a `let`'s `=` (`eq`) and the `else` branch after
/// it, if any. Without an `else`, the initialiser is laid out by
/// [`Formatter::assigned`] — or, as a `let type`'s type (`is_type`), stays
/// on the `=` line as a type alias's does. With one, it stays on the `=`
/// line so that the `else {` is read against the `let`, and the `else` goes
/// after a [`Doc::HANG_LINE`]: on the initialiser's last line when that is
/// the `let`'s own and `else {` fits there, on a line of its own otherwise
/// (a continuation line, or no room), as rustfmt puts it. After a comment
/// that ends the initialiser's line, the `else` starts the next one level
/// in.
fn initialiser<'a>(
    f: &Formatter<'a>,
    eq: TokenIdx,
    init: AnyExpr,
    else_branch: Option<LetElse>,
    is_type: bool,
) -> Doc<'a> {
    let d = f.docs;
    match else_branch {
        Some(else_branch) => {
            let else_doc = d.concat([
                f.keyword(else_branch.kw),
                Doc::SPACE,
                f.expanded_block(f.tree.present(else_branch.block)),
            ]);
            // `else {` on the initialiser's line when it fits there, else on
            // a line of its own — rather than breaking the initialiser to
            // make room for it.
            let else_doc = d.layouts([
                d.concat([Doc::HANG_LINE, else_doc]).broken(),
                d.concat([Doc::HARD_LINE, else_doc]).into(),
            ]);
            d.concat([
                Doc::SPACE,
                d.group(f.alt(init)),
                f.continuation(init.node_id(), else_doc),
            ])
        }
        None if is_type => f.after_operator([eq], init),
        None => f.assigned([eq], init),
    }
}

/// `let pattern = init else { … };`; `semi` is the `;` or, as a C-style
/// `for`'s initialiser, nothing. After a comment that ends the pattern's
/// line, the `=` starts the next one level in.
fn let_stmt<'a>(f: &Formatter<'a>, id: Id<LetStmt>, semi: Doc<'a>) -> Doc<'a> {
    let node = f.tree.get(id);
    let pattern = f.tree.present(node.pattern);
    f.docs.concat([
        f.token(node.introducer),
        Doc::SPACE,
        f.alt(pattern),
        f.continuation(
            pattern.node_id(),
            f.docs.concat([
                Doc::SPACE,
                f.token(f.tree.present(node.eq)),
                initialiser(
                    f,
                    f.tree.present(node.eq).idx,
                    f.tree.present(node.init),
                    node.else_branch,
                    false,
                ),
            ]),
        ),
        semi,
    ])
}

impl FormatNode for LetStmt {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let semi = f.separator(f.tree.get(id).semi, ";");
        let_stmt(f, id, semi)
    }
}

/// `let type T extends A & B = init else { … };` — the type stays on the
/// `=` line, as a type alias's does.
impl FormatNode for TypeBindingStmt {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        let d = f.docs;
        d.concat([
            f.token(node.let_kw),
            Doc::SPACE,
            f.token(node.type_kw),
            Doc::SPACE,
            f.node(f.tree.present(node.name)),
            node.bounds
                .map_or(Doc::NIL, |bounds| d.concat([Doc::SPACE, f.node(bounds)])),
            Doc::SPACE,
            f.token(f.tree.present(node.eq)),
            initialiser(
                f,
                f.tree.present(node.eq).idx,
                f.tree.present(node.init),
                node.else_branch,
                true,
            ),
            f.separator(node.semi, ";"),
        ])
    }
}

/// `defer { … }`; a body that is only a tail may stay on one line, as a
/// block in expression position may.
impl FormatNode for DeferStmt {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        f.docs.concat([
            f.keyword(node.kw),
            Doc::SPACE,
            f.node(f.tree.present(node.body)),
        ])
    }
}

/// A loop or `if` condition, or a `match` scrutinee, in its parentheses,
/// added when the source left them out — laid out as a written pair is
/// ([`ParenExpr`](baml_syntax::nodes::ParenExpr)), so that the next pass,
/// which reads them as one, agrees.
pub(super) fn parenthesised<'a>(f: &Formatter<'a>, cond: AnyExpr) -> Doc<'a> {
    let d = f.docs;
    match cond {
        AnyExpr::Paren(_) => f.alt(cond),
        _ => paren_group(f, None, f.alt(cond), d.text(")")),
    }
}

/// `(inner)`, the inside on a line of its own when it does not fit; `l` is
/// the written `(`, whose comments go inside it, or none for an added one.
fn paren_group<'a>(f: &Formatter<'a>, l: Option<TokenIdx>, inner: Doc<'a>, r: Doc<'a>) -> Doc<'a> {
    let d = f.docs;
    let (comments, line) = match l {
        Some(l) => (f.comments_inside(l), f.line_after_opener(l)),
        None => (Doc::NIL, Doc::SOFT_LINE),
    };
    d.group(d.concat([
        Doc::LINE_SUFFIX_BOUNDARY,
        d.text("("),
        comments,
        d.indent(d.concat([line, inner])),
        Doc::SOFT_LINE,
        r,
    ]))
}

/// `while (cond) { … }`
impl FormatNode for WhileStmt {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        f.docs.concat([
            f.token(node.kw),
            Doc::SPACE,
            parenthesised(f, f.tree.present(node.cond)),
            Doc::HANG_LINE,
            f.expanded_block(f.tree.present(node.body)),
        ])
    }
}

/// `while let pattern = scrutinee { … }`; when the scrutinee's last line is
/// a continuation line, the `{` takes a line of its own ([`Doc::HANG_LINE`]).
impl FormatNode for WhileLetStmt {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        let d = f.docs;
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
        d.concat([
            header,
            Doc::HANG_LINE,
            f.expanded_block(f.tree.present(node.body)),
        ])
    }
}

/// `for (header) { … }`: the header's parentheses are added when the source
/// left them out, and the header goes on a line of its own inside them when
/// it does not fit.
impl FormatNode for ForStmt {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        let d = f.docs;
        let header = match f.tree.present(node.header) {
            AnyForHeader::In(header) => paren_group(f, None, f.node(header), d.text(")")),
            header @ (AnyForHeader::ParenIn(_) | AnyForHeader::CStyle(_)) => f.alt(header),
            AnyForHeader::Bogus(bogus) => f.tree.absent(bogus),
        };
        d.concat([
            f.token(node.kw),
            Doc::SPACE,
            header,
            Doc::HANG_LINE,
            f.expanded_block(f.tree.present(node.body)),
        ])
    }
}

/// `let pattern in iter`
impl FormatNode for ForInHeader {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        f.docs.concat([
            f.token(node.introducer),
            Doc::SPACE,
            f.alt(f.tree.present(node.pattern)),
            Doc::SPACE,
            f.token(f.tree.present(node.in_kw)),
            Doc::SPACE,
            f.alt(f.tree.present(node.iter)),
        ])
    }
}

impl FormatNode for ParenForInHeader {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        paren_group(
            f,
            Some(node.l.idx),
            f.node(node.header),
            f.token(f.tree.present(node.r)),
        )
    }
}

/// `(init; cond; step)`, each part optional: `(;;)` at the least. When it
/// does not fit, each part takes a line of its own inside the parentheses.
impl FormatNode for CForHeader {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        let d = f.docs;
        let init = match node.init {
            Some(AnyForInit::Let(stmt)) => f.with_comments(stmt, let_stmt(f, stmt, Doc::NIL)),
            Some(AnyForInit::Expr(expr)) => f.alt(expr),
            Some(AnyForInit::Bogus(bogus)) => f.tree.absent(bogus),
            None => Doc::NIL,
        };
        let step = match node.step {
            Some(AnyForStep::Assign(stmt)) => d.concat([
                Doc::SOFT_LINE_OR_SPACE,
                f.with_comments(stmt, assign(f, stmt, Doc::NIL)),
            ]),
            Some(AnyForStep::Expr(expr)) => d.concat([Doc::SOFT_LINE_OR_SPACE, f.alt(expr)]),
            Some(AnyForStep::Bogus(bogus)) => f.tree.absent(bogus),
            None => Doc::NIL,
        };
        let l = f.tree.present(node.l).idx;
        d.group(d.concat([
            Doc::LINE_SUFFIX_BOUNDARY,
            d.text("("),
            f.comments_inside(l),
            d.indent(d.concat([
                f.line_after_opener(l),
                init,
                f.token(f.tree.present(node.semi1)),
                node.cond.map_or(Doc::NIL, |cond| {
                    d.concat([Doc::SOFT_LINE_OR_SPACE, f.alt(cond)])
                }),
                f.token(f.tree.present(node.semi2)),
                step,
            ])),
            Doc::SOFT_LINE,
            f.token(f.tree.present(node.r)),
        ]))
    }
}

/// A `//#` section heading, on a line of its own, without its trailing
/// blanks.
impl FormatNode for HeaderStmt {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        super::items::comment_line(f, f.tree.get(id).token)
    }
}
