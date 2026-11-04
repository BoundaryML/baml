//! Blocks and statements. A block's statements are separated by lines; its
//! value is the final expression with no `;`. A `//#` line in a block is a
//! statement of its own.

use baml_syntax::{
    Token,
    nodes::{
        AnyExpr, AnyForHeader, AnyForInit, AnyForStep, AnyStmt, AssignStmt, Block, CForHeader,
        DeferStmt, EmptyStmt, ExprStmt, ForInHeader, ForStmt, GenericParamBounds, HeaderStmt,
        LetElse, LetStmt, ParenForInHeader, TypeBindingStmt, WhileLetStmt, WhileStmt,
    },
    ops::AssignOp,
    token::{
        Ident, TokenIdx, TokenKind, TokenSet as _, ctx, kw, punct,
        sets::{LetIntroducer, TestKw},
    },
    tree::{Id, Missing, TokenRange},
};

use crate::{
    cursor::{Cursor, Mark},
    error::ParseError,
    expr::{ITEM_KEYWORD, Restrictions, at_expression_start, expr, stmt_expr},
    items::{TestPosition, test_def},
    lookahead::{at_binding_intro, looks_like_for_in},
    parser::{Parse, Parser},
    patterns::pattern_after_introducer,
    types::any_type,
};

/// The block its `{` opens, through its `}`. An unclosed block ends at an
/// item keyword, which no statement can begin with.
pub(crate) fn block(p: &mut Parser<'_, '_>, l: Token!["{"]) -> Id<Block> {
    let (stmts, tail) = body_lines(p, None);
    let stmts = p.out.alloc_list(stmts);
    let r = p.expect::<punct::RBrace>();
    p.alloc(Mark::of(l), Block { l, stmts, tail, r })
}

/// The `{ … }` a construct's header is followed by, or the reported `Missing`
/// named `expected`.
pub(crate) fn body_block(p: &mut Parser<'_, '_>, expected: &str) -> Result<Id<Block>, Missing> {
    match p.cur.eat::<punct::LBrace>() {
        Some(l) => Ok(block(p, l)),
        None => Err(p.missing(expected)),
    }
}

/// The lines of a body up to its `}` (or the end of the input, or an item
/// keyword): the statements and the tail expression, if the last line is one
/// with no `;`. `first` is an expression the caller already parsed as the
/// body's first line.
pub(crate) fn body_lines(
    p: &mut Parser<'_, '_>,
    first: Option<AnyExpr>,
) -> (Vec<AnyStmt>, Option<AnyExpr>) {
    let mut stmts = Vec::new();
    if let Some(expr) = first {
        match expr_line(p, expr) {
            Line::Stmt(stmt) => stmts.push(stmt),
            Line::Tail(expr) => return (stmts, Some(expr)),
        }
    }
    loop {
        header_stmts(p, &mut stmts);
        if p.cur.at(TokenKind::RBrace) || p.cur.at_end() || p.cur.at_set(ITEM_KEYWORD) {
            break;
        }
        match stmt(p) {
            Line::Stmt(stmt) => stmts.push(stmt),
            Line::Tail(expr) => return (stmts, Some(expr)),
        }
    }
    (stmts, None)
}

/// The `//#` lines between the last statement and the next token, each a
/// statement.
fn header_stmts(p: &mut Parser<'_, '_>, stmts: &mut Vec<AnyStmt>) {
    let headers: Vec<_> = p.cur.headers_ahead().collect();
    for token in headers {
        let span = TokenRange::new(token.idx, TokenIdx(token.idx.0 + 1));
        stmts.push(AnyStmt::Header(p.out.alloc(HeaderStmt { token }, span)));
    }
}

/// What one line of a block parsed to.
enum Line {
    Stmt(AnyStmt),
    /// An expression with no `;` that the block's `}` follows: its value.
    Tail(AnyExpr),
}

fn stmt(p: &mut Parser<'_, '_>) -> Line {
    let stmt = if let Some(semi) = p.cur.eat::<punct::Semicolon>() {
        AnyStmt::Empty(p.alloc(Mark::of(semi), EmptyStmt { semi }))
    } else if at_let_type(p.cur)
        && let Some(let_kw) = p.cur.eat::<kw::Let>()
        && let Some(type_kw) = p.cur.eat::<ctx::Type>()
    {
        AnyStmt::TypeBinding(TypeBindingStmt::parse(p, (let_kw, type_kw)))
    } else if at_binding_intro(p.cur)
        && let Some(introducer) = p.cur.eat_in::<LetIntroducer>()
    {
        AnyStmt::Let(let_stmt(p, introducer, Semi::Own))
    } else if at_removed_type_binding(p.cur) {
        AnyStmt::Bogus(removed_type_binding(p))
    } else if let Some(kw) = p.cur.eat::<kw::While>() {
        while_stmt(p, kw)
    } else if let Some(kw) = p.cur.eat::<kw::For>() {
        AnyStmt::For(ForStmt::parse(p, kw))
    } else if let Some(kw) = p.cur.eat::<kw::Defer>() {
        AnyStmt::Defer(DeferStmt::parse(p, kw))
    } else if let Some(kw) = p.cur.eat_in::<TestKw>() {
        AnyStmt::Test(test_def(p, kw, TestPosition::Stmt))
    } else if at_expression_start(p.cur) {
        let Ok(expr) = stmt_expr(p, Restrictions::STATEMENT) else {
            unreachable!("an expression starts here");
        };
        return expr_line(p, expr);
    } else {
        AnyStmt::Bogus(p.bogus_one("expression"))
    };
    Line::Stmt(stmt)
}

/// The line an expression makes: an assignment when an assignment operator
/// follows it; the body's tail when nothing but the `}` follows it; else a
/// statement with its `;` if it has one.
fn expr_line(p: &mut Parser<'_, '_>, expr: AnyExpr) -> Line {
    let mark = p.mark_of_any(&expr);
    if let Some(op) = p.cur.eat_op::<AssignOp>() {
        let stmt = assign_stmt(p, mark, expr, op, Restrictions::STATEMENT, Semi::Own);
        return Line::Stmt(AnyStmt::Assign(stmt));
    }
    let semi = p.cur.eat::<punct::Semicolon>();
    let closes = p.cur.at(TokenKind::RBrace) || p.cur.at_end();
    if semi.is_none() && closes && !p.cur.header_ahead() {
        return Line::Tail(expr);
    }
    Line::Stmt(AnyStmt::Expr(p.alloc(mark, ExprStmt { expr, semi })))
}

/// `value;` after `target op`: the value is an expression, so a second
/// assignment operator in it is reported (assignment does not chain).
fn assign_stmt(
    p: &mut Parser<'_, '_>,
    mark: Mark,
    target: AnyExpr,
    op: baml_syntax::ops::AssignOpTok,
    r: Restrictions,
    semi: Semi,
) -> Id<AssignStmt> {
    let value = expr(p, r);
    let semi = match semi {
        Semi::Own => p.cur.eat::<punct::Semicolon>(),
        Semi::Leave => None,
    };
    p.alloc(
        mark,
        AssignStmt {
            target,
            op,
            value,
            semi,
        },
    )
}

/// `let type Name …` — a `let` whose binding is simply named `type` has no
/// name after it.
pub(crate) fn at_let_type(cur: Cursor<'_>) -> bool {
    cur.at(TokenKind::Let) && cur.nth_is(1, TokenKind::Type) && cur.nth_is_ident(2)
}

/// Whether a `let` or assignment statement owns its `;`: not in a C-style
/// `for` header, where the header does (and the step has none).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Semi {
    Own,
    Leave,
}

/// `let pattern = init else { … };` after its `let`/`const`.
pub(crate) fn let_stmt(
    p: &mut Parser<'_, '_>,
    introducer: Token![LetIntroducer],
    semi: Semi,
) -> Id<LetStmt> {
    let pattern = pattern_after_introducer(p, Restrictions::empty());
    let eq = p.expect_as::<punct::Equals>("initializer (=)");
    let init = match eq {
        Ok(_) => expr(p, Restrictions::STATEMENT),
        Err(missing) => Err(missing),
    };
    let else_branch = p.cur.eat::<kw::Else>().map(|kw| {
        let block = if p.cur.at(TokenKind::If) {
            // `else if` has no value to chain through; the `if` is left for
            // the next statement.
            Err(p.missing("block after 'else' (`else if` is not valid in let-else)"))
        } else {
            body_block(p, "block after 'else'")
        };
        LetElse { kw, block }
    });
    let semi = match semi {
        Semi::Own => p.cur.eat::<punct::Semicolon>(),
        Semi::Leave => None,
    };
    p.alloc(
        Mark::of(introducer),
        LetStmt {
            introducer,
            pattern,
            eq,
            init,
            else_branch,
            semi,
        },
    )
}

/// `let type T extends A & B = operand else { … };` — the `else` runs when the
/// type misses a bound, so it is meaningful only on the bounded form.
impl Parse for TypeBindingStmt {
    type Head = (Token!["let"], Token!["type"]);
    fn parse(p: &mut Parser<'_, '_>, (let_kw, type_kw): Self::Head) -> Id<Self> {
        let name = p.expect_name(Ident::KINDS, "type binding name");
        let bounds = p
            .cur
            .eat::<kw::Extends>()
            .map(|kw| GenericParamBounds::parse(p, kw));
        let eq = p.expect_as::<punct::Equals>("'=' and the runtime type");
        let init = match eq {
            Ok(_) => expr(p, Restrictions::STATEMENT),
            Err(missing) => Err(missing),
        };
        let else_branch = if p.cur.at(TokenKind::Else) && bounds.is_none() {
            p.error_here(
                "`else` runs when the type misses a bound; add `extends` bounds or remove the `else` block",
            );
            None
        } else {
            None
        }
        .or_else(|| {
            p.cur.eat::<kw::Else>().map(|kw| {
                let block = body_block(p, "block after 'else'");
                LetElse { kw, block }
            })
        });
        let semi = p.cur.eat::<punct::Semicolon>();
        p.alloc(
            Mark::of(let_kw),
            TypeBindingStmt {
                let_kw,
                type_kw,
                name,
                bounds,
                eq,
                init,
                else_branch,
                semi,
            },
        )
    }
}

/// The removed body-level `type Name = …` statement: `type` is contextual
/// in expressions, so only the complete head claims the position.
fn at_removed_type_binding(cur: Cursor<'_>) -> bool {
    cur.at(TokenKind::Type) && cur.nth_is_ident(1) && cur.nth_is(2, TokenKind::Equals)
}

/// E0098 for a removed `type T = …;` statement, spelling the `let type`
/// replacement. The right-hand side is read as a type only to find where
/// the statement ends; nothing inside it is kept or reported, as the
/// statement's own diagnostic covers it.
fn removed_type_binding(p: &mut Parser<'_, '_>) -> Id<baml_syntax::tree::Bogus> {
    let mark = p.cur.mark();
    let name = p.cur.text_at(1).unwrap_or_default().to_string();
    let rhs = p.probe(|p| {
        p.cur.skip(); // type
        p.cur.skip(); // name
        p.cur.skip(); // =
        let rhs = p.cur.mark();
        let _extent = any_type(p);
        let rhs = p.cur.range_from(rhs);
        p.cur.eat::<punct::Semicolon>();
        rhs
    });
    let rhs = p.cur.text_of_range(rhs).trim();
    let message = match whole_unreflect_operand(rhs) {
        Some(operand) => format!(
            "body-level `type {name} = …` bindings were removed; write \
             `let type {name} = {operand};` instead"
        ),
        // Spelling the replacement verbatim would keep the removed
        // `unreflect` inside it.
        None if rhs.contains("unreflect") => format!(
            "body-level `type {name} = …` bindings were removed; bind each `unreflect` \
             operand with `let type U = <expr>;` first, then write \
             `let type {name} = reflect.Type.of<…>();` naming `U`"
        ),
        None => format!(
            "body-level `type {name} = …` bindings were removed; write \
             `let type {name} = reflect.Type.of<{rhs}>();` instead"
        ),
    };
    let range = p.cur.text_range(p.cur.range_from(mark));
    p.bogus_since(mark, ParseError::RemovedFeature { message, range })
}

/// The operand of a right-hand side that is exactly `unreflect(<operand>)`;
/// a multi-line operand is elided rather than reflowed.
fn whole_unreflect_operand(rhs: &str) -> Option<&str> {
    let inner = rhs
        .strip_prefix("unreflect")?
        .trim_start()
        .strip_prefix('(')?
        .strip_suffix(')')?;
    // `unreflect(a) | unreflect(b)` also starts and ends with the marker's
    // parentheses; only a balanced interior is one operand.
    let mut depth = 0_i32;
    for ch in inner.chars() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth < 0 {
                    return None;
                }
            }
            _ => {}
        }
    }
    let inner = inner.trim();
    (depth == 0 && !inner.is_empty()).then_some(if inner.contains('\n') { "…" } else { inner })
}

/// After `while`: `while c { }` or `while let p = s { }`.
fn while_stmt(p: &mut Parser<'_, '_>, kw: Token!["while"]) -> AnyStmt {
    let r = Restrictions::STATEMENT.before_body();
    if at_binding_intro(p.cur)
        && let Some(introducer) = p.cur.eat_in::<LetIntroducer>()
    {
        // The pattern ends at `=`, so a destructure's `{` is never the body's.
        let pattern = pattern_after_introducer(p, Restrictions::empty());
        let eq = p.expect_as::<punct::Equals>("'=' after while-let pattern");
        let scrutinee = match eq {
            Ok(_) => expr(p, r),
            Err(missing) => Err(missing),
        };
        let body = body_block(p, "block after while-let scrutinee");
        return AnyStmt::WhileLet(p.alloc(
            Mark::of(kw),
            WhileLetStmt {
                kw,
                introducer,
                pattern,
                eq,
                scrutinee,
                body,
            },
        ));
    }
    let cond = if at_expression_start(p.cur) {
        expr(p, r)
    } else {
        Err(p.missing("expression"))
    };
    let body = body_block(p, "block after while condition");
    AnyStmt::While(p.alloc(Mark::of(kw), WhileStmt { kw, cond, body }))
}

/// After `for`: `for (let x in xs) { }`, `for let x in xs { }`, or
/// `for (init; cond; step) { }`.
impl Parse for ForStmt {
    type Head = Token!["for"];
    fn parse(p: &mut Parser<'_, '_>, kw: Self::Head) -> Id<Self> {
        let header = for_header(p, Restrictions::STATEMENT.before_body());
        let body = body_block(p, "block after for expression");
        p.alloc(
            Mark::of(kw),
            ForStmt {
                kw,
                header: Ok(header),
                body,
            },
        )
    }
}

/// After `for`: the header in one of its three shapes. `bare` is how the
/// unparenthesized forms read their expressions (a `{` after them is the
/// loop body, or a template tag's `}` follows).
pub(crate) fn for_header(p: &mut Parser<'_, '_>, bare: Restrictions) -> AnyForHeader {
    if let Some(l) = p.cur.eat::<punct::LParen>() {
        if at_binding_intro(p.cur)
            && looks_like_for_in(p.cur)
            && let Some(introducer) = p.cur.eat_in::<LetIntroducer>()
        {
            let header = for_in_header(p, introducer, Restrictions::empty());
            let r = p.expect::<punct::RParen>();
            return AnyForHeader::ParenIn(p.alloc(Mark::of(l), ParenForInHeader { l, header, r }));
        }
        return AnyForHeader::CStyle(c_for_header(p, Mark::of(l), Ok(l), Restrictions::empty()));
    }
    let mark = p.cur.mark();
    if at_binding_intro(p.cur)
        && looks_like_for_in(p.cur)
        && let Some(introducer) = p.cur.eat_in::<LetIntroducer>()
    {
        return AnyForHeader::In(for_in_header(p, introducer, bare));
    }
    // A C-style header whose `(` is missing: parsed as one, so its parts
    // stay inside the loop.
    let l = Err(p.missing("'('"));
    AnyForHeader::CStyle(c_for_header(p, mark, l, bare))
}

/// `pattern in iterable` after its `let`/`const`, the lookahead having seen
/// the `in`.
fn for_in_header(
    p: &mut Parser<'_, '_>,
    introducer: Token![LetIntroducer],
    r: Restrictions,
) -> Id<ForInHeader> {
    let pattern = pattern_after_introducer(p, Restrictions::empty());
    let in_kw = p.expect::<kw::In>();
    let iter = if at_expression_start(p.cur) {
        expr(p, r)
    } else {
        Err(p.missing("expression"))
    };
    p.alloc(
        Mark::of(introducer),
        ForInHeader {
            introducer,
            pattern,
            in_kw,
            iter,
        },
    )
}

/// `init; cond; step)` after the `(` (`l`), every part optional. With the
/// `(` missing, the `)` is not demanded either.
fn c_for_header(
    p: &mut Parser<'_, '_>,
    mark: Mark,
    l: Result<Token!["("], Missing>,
    r: Restrictions,
) -> Id<CForHeader> {
    let init = if at_binding_intro(p.cur)
        && let Some(introducer) = p.cur.eat_in::<LetIntroducer>()
    {
        Some(AnyForInit::Let(let_stmt(p, introducer, Semi::Leave)))
    } else if at_expression_start(p.cur) {
        expr(p, r).ok().map(AnyForInit::Expr)
    } else {
        None
    };
    let semi1 = p.cur.eat::<punct::Semicolon>();
    let cond = (!p.cur.at(TokenKind::LBrace) && at_expression_start(p.cur))
        .then(|| expr(p, r).ok())
        .flatten();
    let semi2 = p.cur.eat::<punct::Semicolon>();
    let step = (!p.cur.at(TokenKind::LBrace) && at_expression_start(p.cur))
        .then(|| {
            let target = stmt_expr(p, r).ok()?;
            Some(match p.cur.eat_op::<AssignOp>() {
                Some(op) => {
                    let mark = p.mark_of_any(&target);
                    AnyForStep::Assign(assign_stmt(p, mark, target, op, r, Semi::Leave))
                }
                None => AnyForStep::Expr(target),
            })
        })
        .flatten();
    let close = match l {
        Ok(_) => p.expect::<punct::RParen>(),
        Err(missing) => p.cur.eat::<punct::RParen>().ok_or(missing),
    };
    p.alloc(
        mark,
        CForHeader {
            l,
            init,
            semi1,
            cond,
            semi2,
            step,
            r: close,
        },
    )
}

/// `defer { … }`: the body is always a block.
impl Parse for DeferStmt {
    type Head = Token!["defer"];
    fn parse(p: &mut Parser<'_, '_>, kw: Self::Head) -> Id<Self> {
        let body = body_block(p, "'{' after defer");
        p.alloc(Mark::of(kw), DeferStmt { kw, body })
    }
}
