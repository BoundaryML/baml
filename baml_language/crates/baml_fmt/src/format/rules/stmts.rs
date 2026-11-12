//! Statements and blocks: one statement per line, every statement closed by
//! `;` — except one that ends in a block, which keeps the `;` it was
//! written with, since as the last statement a `;` is what makes it a
//! statement rather than the block's tail — a stray `;` printing nothing, a
//! block's tail bare. Loop conditions and `for` headers are parenthesised; a block in
//! statement position is always broken, and a block that is only a tail may
//! stay on one line elsewhere.

use baml_syntax::{
    nodes::{
        AnyExpr, AnyForHeader, AnyForInit, AnyForStep, AssignStmt, Block, CForHeader, DeferStmt,
        EmptyStmt, ExprStmt, ForInHeader, ForStmt, HeaderStmt, LetElse, LetStmt, ParenForInHeader,
        TypeBindingStmt, WhileLetStmt, WhileStmt,
    },
    tree::Id,
};

use crate::{
    format::{FormatNode, Formatter},
    ir::Doc,
};

/// A block in expression position: `{ x }` when it is only a tail and fits.
impl FormatNode for Block {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        let tail_only = f.tree.list(node.stmts).is_empty() && node.tail.is_some();
        f.block_parts(id, tail_only)
    }
}

/// Whether an expression in statement position ends in a block, so that its
/// `;` is kept as written rather than added.
fn ends_in_block(expr: AnyExpr) -> bool {
    matches!(
        expr,
        AnyExpr::If(_)
            | AnyExpr::IfLet(_)
            | AnyExpr::Match(_)
            | AnyExpr::Block(_)
            | AnyExpr::Catch(_)
    )
}

impl FormatNode for ExprStmt {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        let semi = if ends_in_block(node.expr) {
            node.semi.map_or(Doc::NIL, |semi| f.token(semi))
        } else {
            f.separator(node.semi, ";")
        };
        f.docs.concat([f.alt(node.expr), semi])
    }
}

/// `target op value;`; `semi` is the `;` or, in a `for` step, nothing.
fn assign<'a>(f: &Formatter<'a>, id: Id<AssignStmt>, semi: Doc<'a>) -> Doc<'a> {
    let node = f.tree.get(id);
    let d = f.docs;
    d.concat([
        f.alt(node.target),
        Doc::SPACE,
        d.text(node.op.op().spelling()),
        d.concat(node.op.tokens().map(|token| f.comments_after(token))),
        Doc::SPACE,
        f.alt(f.tree.present(node.value)),
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

/// The initialiser after a `let`'s `=` and the `else` branch after it, if
/// any. The initialiser stays on the `=` line; with an `else`, when it broke
/// over several lines the `else {` takes a line of its own, as rustfmt puts
/// it, so that it is read against the `let`.
fn initialiser<'a>(f: &Formatter<'a>, init: AnyExpr, else_branch: Option<LetElse>) -> Doc<'a> {
    let d = f.docs;
    match else_branch {
        Some(else_branch) => {
            let (init, group) = d.group_with_id(f.alt(init));
            d.concat([
                Doc::SPACE,
                init,
                f.brace_after(Some(group)),
                f.token(else_branch.kw),
                Doc::SPACE,
                f.expanded_block(f.tree.present(else_branch.block)),
            ])
        }
        None => d.concat([Doc::SPACE, f.alt(init)]),
    }
}

/// `let pattern = init else { … };`; `semi` is the `;` or, as a C-style
/// `for`'s initialiser, nothing.
fn let_stmt<'a>(f: &Formatter<'a>, id: Id<LetStmt>, semi: Doc<'a>) -> Doc<'a> {
    let node = f.tree.get(id);
    f.docs.concat([
        f.token(node.introducer),
        Doc::SPACE,
        f.alt(f.tree.present(node.pattern)),
        Doc::SPACE,
        f.token(f.tree.present(node.eq)),
        initialiser(f, f.tree.present(node.init), node.else_branch),
        semi,
    ])
}

impl FormatNode for LetStmt {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let semi = f.separator(f.tree.get(id).semi, ";");
        let_stmt(f, id, semi)
    }
}

/// `let type T extends A & B = init else { … };`
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
            initialiser(f, f.tree.present(node.init), node.else_branch),
            f.separator(node.semi, ";"),
        ])
    }
}

impl FormatNode for DeferStmt {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        f.docs.concat([
            f.token(node.kw),
            Doc::SPACE,
            f.expanded_block(f.tree.present(node.body)),
        ])
    }
}

/// A loop condition in its parentheses, added when the source left them
/// out.
fn parenthesised<'a>(f: &Formatter<'a>, cond: AnyExpr) -> Doc<'a> {
    match cond {
        AnyExpr::Paren(_) => f.alt(cond),
        _ => f
            .docs
            .concat([f.docs.text("("), f.alt(cond), f.docs.text(")")]),
    }
}

/// `while (cond) { … }`
impl FormatNode for WhileStmt {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        f.docs.concat([
            f.token(node.kw),
            Doc::SPACE,
            parenthesised(f, f.tree.present(node.cond)),
            Doc::SPACE,
            f.expanded_block(f.tree.present(node.body)),
        ])
    }
}

/// `while let pattern = scrutinee { … }`; when the scrutinee broke over
/// several lines, the `{` takes a line of its own.
impl FormatNode for WhileLetStmt {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        let d = f.docs;
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
        d.concat([
            header,
            f.brace_after(Some(group)),
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
            AnyForHeader::In(header) => paren_group(f, d.text("("), f.node(header), d.text(")")),
            header @ (AnyForHeader::ParenIn(_) | AnyForHeader::CStyle(_)) => f.alt(header),
            AnyForHeader::Bogus(bogus) => f.tree.absent(bogus),
        };
        d.concat([
            f.token(node.kw),
            Doc::SPACE,
            header,
            Doc::SPACE,
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
            f.token(node.l),
            f.node(node.header),
            f.token(f.tree.present(node.r)),
        )
    }
}

/// `(inner)`, the inside on a line of its own when it does not fit.
fn paren_group<'a>(f: &Formatter<'a>, l: Doc<'a>, inner: Doc<'a>, r: Doc<'a>) -> Doc<'a> {
    let d = f.docs;
    d.group(d.concat([
        l,
        d.indent(d.concat([Doc::SOFT_LINE, inner])),
        Doc::SOFT_LINE,
        r,
    ]))
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
        d.group(d.concat([
            f.token(f.tree.present(node.l)),
            d.indent(d.concat([
                Doc::SOFT_LINE,
                init,
                f.separator(node.semi1, ";"),
                node.cond.map_or(Doc::NIL, |cond| {
                    d.concat([Doc::SOFT_LINE_OR_SPACE, f.alt(cond)])
                }),
                f.separator(node.semi2, ";"),
                step,
            ])),
            Doc::SOFT_LINE,
            f.token(f.tree.present(node.r)),
        ]))
    }
}

/// A `//#` section heading, on a line of its own.
impl FormatNode for HeaderStmt {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        f.token(f.tree.get(id).token)
    }
}
