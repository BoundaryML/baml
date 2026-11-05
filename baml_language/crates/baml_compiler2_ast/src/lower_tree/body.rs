//! Expression bodies: a typed [`Block`] → `ExprBody` + `AstSourceMap`, in the
//! same arenas the rowan lowering fills. The arenas, the source map and the
//! desugaring helpers are `LoweringContext`'s; this module reads the typed
//! tree into them. Allocation order is the rowan lowering's — children in
//! source order, then the parent — so the two bodies compare equal id for id.

use baml_base::{Literal, Name, num_lit};
use baml_syntax::{
    nodes::{
        AnyArrayPatternElement, AnyElse, AnyExpr, AnyForHeader, AnyForInit, AnyForStep,
        AnyMatchScrutinee, AnyObjectCtor, AnyObjectKey, AnyObjectMember, AnyPattern, AnyStmt,
        AnyTypeArg, ArgList, ArrayPattern, AssignStmt, BindingPattern, Block, DestructurePattern,
        FileTree, GenericArgs, HeaderStmt, IfExpr, IfLetExpr, LambdaExpr, LetStmt, ObjectLiteral,
        ParamList, PathExpr, StringLit, UnaryExpr,
    },
    ops,
    token::TokenKind,
    tree::{Id, List, Missing},
};
use text_size::TextRange;

use super::{View, templates, types};
use crate::{
    EnvVarRef, LoweringDiagnostic,
    ast::{
        ArrayRestPat, AssignOp, AstSourceMap, BinaryOp, CallArg, CatchArm, CatchClause,
        CatchClauseKind, DefaultExprId, Expr, ExprBody, ExprId, FieldPat, FunctionDefaults,
        LambdaDef, LambdaKind, LetOrigin, LoopOrigin, MapExprEntry, MatchArm, ObjectExprField,
        Param, PatId, Pattern, SpreadField, Stmt, StmtId, TypeExpr, TypeExprKind, UnaryOp,
    },
    lower_cst::check_missing_type,
    lower_expr_body::{LoweringContext, strip_raw_string_delimiters, strip_string_delimiters},
    lower_type_expr::check_wildcard_type,
};

/// Lower a function body's block.
pub fn lower_body(
    tree: &FileTree,
    block: Id<Block>,
) -> (
    ExprBody,
    AstSourceMap,
    Vec<LoweringDiagnostic>,
    Vec<EnvVarRef>,
) {
    let v = View::new(tree);
    let mut ctx = LoweringContext::new();
    let root = block_expr(&mut ctx, v, block);
    ctx.finish(Some(root))
}

// ── Blocks and statements ────────────────────────────────────────────────────

fn block_expr(ctx: &mut LoweringContext, v: View<'_>, id: Id<Block>) -> ExprId {
    let node = *v.node(id);
    block_body(ctx, v, node.stmts, node.tail, v.range_of(id))
}

/// A block's statements and tail as one `Expr::Block`; a `${…}`
/// interpolation is one without its own node.
pub(super) fn block_body(
    ctx: &mut LoweringContext,
    v: View<'_>,
    stmts: List<AnyStmt>,
    tail: Option<AnyExpr>,
    span: TextRange,
) -> ExprId {
    // A header after the block's last expression makes that expression a
    // statement (ruled 2026-11-04): the block then has no value, and a
    // context that needs one reports it.
    let mut stmts_out = Vec::new();
    for &stmt in v.tree.list(stmts) {
        stmts_out.extend(statement(ctx, v, stmt));
    }
    // A diverging tail (`return x`, `throw e`, `break`, `continue`) is the
    // statement it is in the rowan lowering: the block has no value.
    let tail_expr = match tail {
        Some(AnyExpr::Return(id)) => {
            let expr = *v.node(id);
            let value = expr.value.map(|value| expression(ctx, v, value));
            stmts_out.push(ctx.alloc_stmt(Stmt::Return(value), v.range_of(id)));
            None
        }
        Some(AnyExpr::Throw(id)) => {
            let value = expr_or_missing(ctx, v, v.node(id).payload, v.range_of(id));
            stmts_out.push(ctx.alloc_stmt(Stmt::Throw { value }, v.range_of(id)));
            None
        }
        Some(AnyExpr::Break(id)) => {
            stmts_out.push(ctx.alloc_stmt(Stmt::Break, v.range_of(id)));
            None
        }
        Some(AnyExpr::Continue(id)) => {
            stmts_out.push(ctx.alloc_stmt(Stmt::Continue, v.range_of(id)));
            None
        }
        Some(tail) => Some(expression(ctx, v, tail)),
        None => None,
    };
    ctx.alloc_expr(
        Expr::Block {
            stmts: stmts_out,
            tail_expr,
        },
        span,
    )
}

/// One statement; a stray `;` is none.
fn statement(ctx: &mut LoweringContext, v: View<'_>, stmt: AnyStmt) -> Option<StmtId> {
    Some(match stmt {
        AnyStmt::Let(id) => let_stmt(ctx, v, id),
        AnyStmt::Assign(id) => assignment(ctx, v, id),
        AnyStmt::Expr(id) => {
            let node = *v.node(id);
            let span = v.range_of(id);
            match node.expr {
                // Diverging forms are statements of their own kind.
                AnyExpr::Return(id) => {
                    let value = v.node(id).value.map(|value| expression(ctx, v, value));
                    ctx.alloc_stmt(Stmt::Return(value), span)
                }
                AnyExpr::Throw(id) => {
                    let value = expr_or_missing(ctx, v, v.node(id).payload, v.range_of(id));
                    ctx.alloc_stmt(Stmt::Throw { value }, span)
                }
                AnyExpr::Break(_) => ctx.alloc_stmt(Stmt::Break, span),
                AnyExpr::Continue(_) => ctx.alloc_stmt(Stmt::Continue, span),
                expr => {
                    // The statement spans its expression, not its `;`.
                    let expr_span = v.range_any(&expr);
                    let expr = expression(ctx, v, expr);
                    ctx.alloc_stmt(Stmt::Expr(expr), expr_span)
                }
            }
        }
        AnyStmt::Empty(_) => return None,
        AnyStmt::Defer(id) => {
            let node = *v.node(id);
            let span = v.range_of(id);
            let body = match node.body {
                Ok(block) => block_expr(ctx, v, block),
                Err(_) => ctx.alloc_expr(Expr::Missing, span),
            };
            ctx.alloc_stmt(Stmt::Defer { body }, span)
        }
        AnyStmt::While(id) => {
            let node = *v.node(id);
            let span = v.range_of(id);
            let condition = expr_or_missing(ctx, v, node.cond, span);
            let body = block_or_missing(ctx, v, node.body, span);
            ctx.alloc_stmt(
                Stmt::While {
                    condition,
                    body,
                    after: None,
                    origin: LoopOrigin::While,
                },
                span,
            )
        }
        AnyStmt::For(id) => for_stmt(ctx, v, id),
        AnyStmt::Header(id) => header_stmt(ctx, v, id),
        AnyStmt::TypeBinding(id) => {
            let node = *v.node(id);
            let span = v.range_of(id);
            let name = node
                .name
                .ok()
                .map_or_else(|| Name::new("<missing>"), |name| v.name(name));
            let mut bounds = Vec::new();
            if let Some(id) = node.bounds {
                let node = *v.node(id);
                let arms: Vec<_> = v.tree.list(node.rest).to_vec();
                let tys = std::iter::once(node.first).chain(arms.into_iter().map(|arm| arm.ty));
                for ty in tys.flatten() {
                    bounds.push(types::lower_type(v, &mut ctx.diags, ty));
                }
            }
            let operand = expr_or_missing(ctx, v, node.init, span);
            let else_branch = node
                .else_branch
                .and_then(|else_branch| else_branch.block.ok())
                .map(|block| block_expr(ctx, v, block));
            let stmt = ctx.alloc_stmt(
                Stmt::TypeBinding {
                    name,
                    operand,
                    bounds,
                    else_branch,
                },
                span,
            );
            if let Ok(name) = node.name {
                ctx.source_map
                    .type_binding_name_spans
                    .insert(stmt, v.name_range(name));
            }
            stmt
        }
        AnyStmt::WhileLet(id) => {
            let node = *v.node(id);
            let span = v.range_of(id);
            warn_const_introducer(ctx, v, node.introducer);
            let pattern = match node.pattern {
                Ok(pattern) => self::pattern(ctx, v, pattern),
                Err(_) => ctx.alloc_pattern(Pattern::Wildcard, span),
            };
            let scrutinee = expr_or_missing(ctx, v, node.scrutinee, span);
            let body = block_or_missing(ctx, v, node.body, span);
            ctx.alloc_stmt(
                Stmt::WhileLet {
                    pattern,
                    scrutinee,
                    body,
                },
                span,
            )
        }
        AnyStmt::Test(id) => ctx.alloc_stmt(Stmt::Missing, v.range_of(id)),
        AnyStmt::Bogus(id) => ctx.alloc_stmt(Stmt::Missing, v.range_of(id)),
    })
}

pub(super) fn let_stmt(ctx: &mut LoweringContext, v: View<'_>, id: Id<LetStmt>) -> StmtId {
    let node = *v.node(id);
    let span = v.range_of(id);
    let pattern = node
        .pattern
        .ok()
        .map(|pattern| self::pattern(ctx, v, pattern));
    let initializer = node.init.ok().map(|init| expression(ctx, v, init));
    let else_branch = node
        .else_branch
        .and_then(|else_branch| else_branch.block.ok())
        .map(|block| block_expr(ctx, v, block));
    warn_const_introducer(ctx, v, node.introducer);
    let pattern = pattern.unwrap_or_else(|| ctx.alloc_pattern(Pattern::Wildcard, span));
    ctx.alloc_stmt(
        Stmt::Let {
            pattern,
            initializer,
            origin: LetOrigin::Source,
            else_branch,
        },
        span,
    )
}

/// A `const` introducer is reported where it is not the file-scope one.
pub(super) fn warn_const_introducer(
    ctx: &mut LoweringContext,
    v: View<'_>,
    introducer: baml_syntax::token::Token<baml_syntax::token::sets::LetIntroducer>,
) {
    if v.tree.tokens().kind(introducer.idx) != TokenKind::Const {
        return;
    }
    let span = v.token_range(introducer.idx);
    if ctx.file_scope_introducer != Some(span) {
        ctx.warn_const_introducer(span);
    }
}

/// `target = value` or `target op= value`. The statement spans the
/// assignment, not its `;`, as the rowan lowering spans it.
pub(super) fn assignment(ctx: &mut LoweringContext, v: View<'_>, id: Id<AssignStmt>) -> StmtId {
    let node = *v.node(id);
    let op = match node.op.op() {
        ops::AssignOp::Assign => None,
        ops::AssignOp::AddAssign => Some(AssignOp::Add),
        ops::AssignOp::SubAssign => Some(AssignOp::Sub),
        ops::AssignOp::MulAssign => Some(AssignOp::Mul),
        ops::AssignOp::DivAssign => Some(AssignOp::Div),
        ops::AssignOp::ModAssign => Some(AssignOp::Mod),
        ops::AssignOp::BitAndAssign => Some(AssignOp::BitAnd),
        ops::AssignOp::BitOrAssign => Some(AssignOp::BitOr),
        ops::AssignOp::BitXorAssign => Some(AssignOp::BitXor),
        ops::AssignOp::ShlAssign => Some(AssignOp::Shl),
        ops::AssignOp::ShrAssign => Some(AssignOp::Shr),
    };
    let start = v.range_any(&node.target).start();
    let end = match node.value {
        Ok(value) => v.range_any(&value).end(),
        Err(_) => {
            let last = node
                .op
                .tokens()
                .last()
                .unwrap_or_else(|| unreachable!("an operator has a token"));
            v.token_range(last).end()
        }
    };
    let span = TextRange::new(start, end);
    let target = expression(ctx, v, node.target);
    let value = expr_or_missing(ctx, v, node.value, span);
    // Only a place can be assigned to; `Missing` counts so a reported syntax
    // error does not cascade.
    if !ctx.is_assignable_place(target) {
        let destructure =
            op.is_none() && matches!(ctx.exprs[target], Expr::Array { .. } | Expr::Object { .. });
        ctx.diags.push(LoweringDiagnostic::InvalidAssignmentTarget {
            span: ctx.source_map.expr_span(target),
            destructure,
        });
    }
    let stmt = match op {
        None => Stmt::Assign { target, value },
        Some(op) => Stmt::AssignOp { target, op, value },
    };
    ctx.alloc_stmt(stmt, span)
}

fn for_stmt(ctx: &mut LoweringContext, v: View<'_>, id: Id<baml_syntax::nodes::ForStmt>) -> StmtId {
    let node = *v.node(id);
    let span = v.range_of(id);
    let header = match node.header {
        Ok(AnyForHeader::In(header)) => Some(*v.node(header)),
        Ok(AnyForHeader::ParenIn(paren)) => Some(*v.node(v.node(paren).header)),
        Ok(AnyForHeader::CStyle(header)) => return c_style_for(ctx, v, header, node.body, span),
        Ok(AnyForHeader::Bogus(_)) | Err(_) => None,
    };
    let binding = header.and_then(|header| {
        warn_const_introducer(ctx, v, header.introducer);
        header
            .pattern
            .ok()
            .map(|pattern| self::pattern(ctx, v, pattern))
    });
    let collection = header
        .and_then(|header| header.iter.ok())
        .map(|iter| expression(ctx, v, iter));
    let body = node.body.ok().map(|block| block_expr(ctx, v, block));
    let collection = collection.unwrap_or_else(|| ctx.alloc_expr(Expr::Missing, span));
    let body = body.unwrap_or_else(|| ctx.alloc_expr(Expr::Missing, span));
    let binding = binding.unwrap_or_else(|| ctx.alloc_pattern(Pattern::Wildcard, span));
    ctx.alloc_stmt(
        Stmt::For {
            binding,
            collection,
            body,
        },
        span,
    )
}

/// `for (let i = 0; cond; step) { body }` as `{ let i = 0; while cond { body }
/// after { step } }`, as the rowan lowering desugars it.
fn c_style_for(
    ctx: &mut LoweringContext,
    v: View<'_>,
    header: Id<baml_syntax::nodes::CForHeader>,
    body: Result<Id<Block>, Missing>,
    span: TextRange,
) -> StmtId {
    let header = *v.node(header);
    let init_stmt = match header.init {
        Some(AnyForInit::Let(init)) => let_stmt(ctx, v, init),
        Some(AnyForInit::Expr(_) | AnyForInit::Bogus(_)) | None => {
            ctx.alloc_stmt(Stmt::Missing, span)
        }
    };
    let condition = match header.cond {
        Some(cond) => expression(ctx, v, cond),
        None => ctx.alloc_expr(Expr::Missing, span),
    };
    let after_stmt = header.step.map(|step| step_stmt(ctx, v, step));
    let body = block_or_missing(ctx, v, body, span);
    let while_stmt = ctx.alloc_stmt(
        Stmt::While {
            condition,
            body,
            after: after_stmt,
            origin: LoopOrigin::For { init: init_stmt },
        },
        span,
    );
    let block = ctx.alloc_expr(
        Expr::Block {
            stmts: vec![init_stmt, while_stmt],
            tail_expr: None,
        },
        span,
    );
    ctx.alloc_stmt(Stmt::Expr(block), span)
}

/// A C-style `for` step: an assignment (`i += 1`) or an expression run for
/// its effect.
pub(super) fn step_stmt(ctx: &mut LoweringContext, v: View<'_>, step: AnyForStep) -> StmtId {
    match step {
        AnyForStep::Assign(id) => assignment(ctx, v, id),
        AnyForStep::Expr(step) => {
            let span = v.range_any(&step);
            let expr = expression(ctx, v, step);
            ctx.alloc_stmt(Stmt::Expr(expr), span)
        }
        AnyForStep::Bogus(bogus) => ctx.alloc_stmt(Stmt::Missing, v.range_of(bogus)),
    }
}

/// `//# Title`: the level is the number of `#`s, the name the rest.
fn header_stmt(ctx: &mut LoweringContext, v: View<'_>, id: Id<HeaderStmt>) -> StmtId {
    let text = v.text(v.node(id).token.idx);
    let after_slashes = text.strip_prefix("//").unwrap_or(text);
    let level = after_slashes.chars().take_while(|c| *c == '#').count();
    let title = after_slashes[level..].trim();
    let name = Name::new(if title.is_empty() { "_" } else { title });
    ctx.alloc_stmt(Stmt::HeaderComment { name, level }, v.range_of(id))
}

// ── Expressions ──────────────────────────────────────────────────────────────

/// An expression; one holding an unwrapped `?.` chain is wrapped here.
pub(super) fn expression(ctx: &mut LoweringContext, v: View<'_>, expr: AnyExpr) -> ExprId {
    let id = expression_in_chain(ctx, v, expr);
    if ctx.needs_chain_wrap.remove(&id) {
        ctx.alloc_expr(Expr::OptionalChain { expr: id }, v.range_any(&expr))
    } else {
        id
    }
}

pub(super) fn expr_or_missing(
    ctx: &mut LoweringContext,
    v: View<'_>,
    expr: Result<AnyExpr, Missing>,
    missing_at: TextRange,
) -> ExprId {
    match expr {
        Ok(expr) => expression(ctx, v, expr),
        Err(_) => ctx.alloc_expr(Expr::Missing, missing_at),
    }
}

fn block_or_missing(
    ctx: &mut LoweringContext,
    v: View<'_>,
    block: Result<Id<Block>, Missing>,
    missing_at: TextRange,
) -> ExprId {
    match block {
        Ok(block) => block_expr(ctx, v, block),
        Err(_) => ctx.alloc_expr(Expr::Missing, missing_at),
    }
}

/// An expression as the base of a `?.` chain: not wrapped, so the chain
/// closes at its outermost link.
fn expression_in_chain(ctx: &mut LoweringContext, v: View<'_>, expr: AnyExpr) -> ExprId {
    let span = v.range_any(&expr);
    match expr {
        AnyExpr::Path(id) => path_expr(ctx, v, id),
        AnyExpr::GenericApply(id) => {
            let node = *v.node(id);
            let base = expression_in_chain(ctx, v, node.base);
            generic_apply(ctx, v, node.base, base, node.args, span)
        }
        AnyExpr::Literal(id) => {
            let token = v.node(id).token;
            let expr = literal_token(ctx, v, token.idx);
            ctx.alloc_expr(expr, span)
        }
        AnyExpr::String(id) => string_literal(ctx, v, id),
        AnyExpr::FieldAccess(id) => {
            let node = *v.node(id);
            let base = expression_in_chain(ctx, v, node.base);
            let member = node
                .name
                .ok()
                .map(|name| (v.name(name), v.name_range(name)));
            let id = ctx.alloc_expr(
                Expr::MemberAccess {
                    base,
                    member: member
                        .as_ref()
                        .map_or_else(|| Name::new("_"), |(n, _)| n.clone()),
                },
                span,
            );
            if let Some((_, range)) = member {
                ctx.source_map.member_access_member_spans.insert(id, range);
            }
            if ctx.needs_chain_wrap.remove(&base) {
                ctx.needs_chain_wrap.insert(id);
            }
            id
        }
        AnyExpr::OptionalFieldAccess(id) => {
            let node = *v.node(id);
            let base = expression_in_chain(ctx, v, node.base);
            let member = node
                .name
                .ok()
                .map(|name| (v.name(name), v.name_range(name)));
            let id = ctx.alloc_expr(
                Expr::OptionalMemberAccess {
                    base,
                    member: member
                        .as_ref()
                        .map_or_else(|| Name::new("_"), |(n, _)| n.clone()),
                },
                span,
            );
            if let Some((_, range)) = member {
                ctx.source_map.member_access_member_spans.insert(id, range);
            }
            ctx.needs_chain_wrap.remove(&base);
            ctx.needs_chain_wrap.insert(id);
            id
        }
        AnyExpr::Call(id) => call_expr(ctx, v, id),
        AnyExpr::OptionalCall(id) => {
            let node = *v.node(id);
            let callee = expression_in_chain(ctx, v, node.callee);
            let (args, label_spans) = call_args(ctx, v, node.args);
            let id = ctx.alloc_expr(Expr::OptionalCall { callee, args }, span);
            ctx.record_call_arg_label_spans(id, label_spans);
            ctx.needs_chain_wrap.remove(&callee);
            ctx.needs_chain_wrap.insert(id);
            id
        }
        AnyExpr::Index(id) => {
            let node = *v.node(id);
            let base = expression_in_chain(ctx, v, node.base);
            let index = expr_or_missing(ctx, v, node.index, span);
            let id = ctx.alloc_expr(Expr::Index { base, index }, span);
            if ctx.needs_chain_wrap.remove(&base) {
                ctx.needs_chain_wrap.insert(id);
            }
            id
        }
        AnyExpr::OptionalIndex(id) => {
            let node = *v.node(id);
            let base = expression_in_chain(ctx, v, node.base);
            let index = expr_or_missing(ctx, v, node.index, span);
            let id = ctx.alloc_expr(Expr::OptionalIndex { base, index }, span);
            ctx.needs_chain_wrap.remove(&base);
            ctx.needs_chain_wrap.insert(id);
            id
        }
        AnyExpr::Binary(id) => binary_expr(ctx, v, id),
        AnyExpr::Unary(id) => unary_expr(ctx, v, id),
        AnyExpr::Await(id) => {
            let node = *v.node(id);
            let future = expr_or_missing(ctx, v, node.operand, span);
            ctx.alloc_expr(Expr::Await { future }, span)
        }
        AnyExpr::Throw(id) => {
            let value = expr_or_missing(ctx, v, v.node(id).payload, span);
            ctx.alloc_expr(Expr::Throw { value }, span)
        }
        AnyExpr::Return(id) => {
            let value = v.node(id).value.map(|value| expression(ctx, v, value));
            ctx.alloc_expr(Expr::Return { value }, span)
        }
        AnyExpr::Break(_) => ctx.lower_jump_expr_at(Stmt::Break, span),
        AnyExpr::Continue(_) => ctx.lower_jump_expr_at(Stmt::Continue, span),
        AnyExpr::Paren(id) => match v.node(id).inner {
            Ok(inner) => expression(ctx, v, inner),
            Err(_) => ctx.alloc_expr(Expr::Missing, span),
        },
        AnyExpr::Block(id) => block_expr(ctx, v, id),
        AnyExpr::If(id) => if_expr(ctx, v, id),
        AnyExpr::IfLet(id) => if_let_expr(ctx, v, id),
        AnyExpr::Match(id) => match_expr(ctx, v, id),
        AnyExpr::Is(id) => {
            let node = *v.node(id);
            let scrutinee = expression(ctx, v, node.expr);
            let pattern = match node.pattern {
                Ok(pattern) => self::pattern(ctx, v, pattern),
                Err(_) => ctx.alloc_pattern(Pattern::Wildcard, span),
            };
            ctx.alloc_expr(Expr::Is { scrutinee, pattern }, span)
        }
        AnyExpr::Upcast(id) => {
            let node = *v.node(id);
            let base = expression_in_chain(ctx, v, node.base);
            let target = node
                .args
                .ok()
                .and_then(|args| v.tree.list(v.node(args).args).first().map(|item| item.item))
                .map_or_else(
                    || TypeExprKind::Missing.at(span),
                    |ty| types::lower_type(v, &mut ctx.diags, ty),
                );
            let id = ctx.alloc_expr(Expr::Upcast { base, target }, span);
            if ctx.needs_chain_wrap.remove(&base) {
                ctx.needs_chain_wrap.insert(id);
            }
            id
        }
        AnyExpr::QualifiedPath(id) => {
            let node = *v.node(id);
            let qself = types::lower_type(v, &mut ctx.diags, node.base);
            let interface = match node.iface {
                Ok(ty) => types::lower_type(v, &mut ctx.diags, ty),
                Err(_) => TypeExprKind::Missing.at(span),
            };
            let Ok(member) = node.member else {
                return ctx.alloc_expr(Expr::Missing, span);
            };
            let id = ctx.alloc_expr(
                Expr::QualifiedPath {
                    qself,
                    interface,
                    member: v.name(member),
                },
                span,
            );
            ctx.source_map
                .member_access_member_spans
                .insert(id, v.name_range(member));
            id
        }
        // `env.X` is a late-bound reference: `baml.env.ref("X")`.
        AnyExpr::EnvAccess(id) => {
            let node = *v.node(id);
            let var_name = node
                .name
                .ok()
                .map_or_else(|| "_".to_string(), |name| v.name(name).as_str().to_string());
            ctx.env_var_refs.push(EnvVarRef {
                name: var_name.clone(),
                range: span,
            });
            let callee = ctx.alloc_expr(Expr::Path(baml_base::compiler_path!(baml.env.ref)), span);
            let arg = ctx.alloc_expr(Expr::Literal(Literal::String(var_name)), span);
            ctx.alloc_expr(
                Expr::Call {
                    callee,
                    type_args: vec![],
                    args: vec![CallArg::positional(arg)],
                },
                span,
            )
        }
        // `F@spec`: the companion's own name, `F@spec`, as a path.
        AnyExpr::Spec(id) => {
            let node = *v.node(id);
            let (AnyExpr::Path(base), Ok(selector)) = (node.base, node.selector) else {
                ctx.diags.push(LoweringDiagnostic::UnparseableType {
                    context: "companion target (expected an LLM function reference)".to_string(),
                    span,
                });
                return ctx.alloc_expr(Expr::Missing, span);
            };
            let base = *v.node(base);
            let mut segments = vec![v.name(base.head)];
            for segment in v.tree.list(base.segments) {
                let Ok(name) = segment.name else { break };
                segments.push(v.name(name));
            }
            let last = segments
                .pop()
                .unwrap_or_else(|| unreachable!("a path has a head"));
            let suffix = v.text(selector.idx);
            segments.push(Name::new(format!("{}@{suffix}", last.as_str())));
            ctx.alloc_expr(Expr::Path(baml_base::Path::new(segments)), span)
        }
        AnyExpr::Lambda(id) => lambda_expr(ctx, v, id),
        AnyExpr::Object(id) => object_literal(ctx, v, id),
        AnyExpr::Catch(id) => catch_expr(ctx, v, id),
        AnyExpr::Spawn(id) => spawn_expr(ctx, v, id),
        AnyExpr::Array(id) => {
            let node = *v.node(id);
            let elements: Vec<_> = v.tree.list(node.elements).to_vec();
            let elements = elements
                .into_iter()
                .map(|element| expression(ctx, v, element.value))
                .collect();
            ctx.alloc_expr(Expr::Array { elements }, span)
        }
        AnyExpr::Map(id) => {
            let node = *v.node(id);
            let entries: Vec<_> = v.tree.list(node.entries).to_vec();
            let entries = entries
                .into_iter()
                .filter_map(|entry| {
                    let entry = *v.node(entry);
                    let key = expression(ctx, v, entry.key);
                    // An entry with no value is not one, as in the rowan
                    // lowering (the parser reported it).
                    let value = expression(ctx, v, entry.value.ok()?);
                    Some(MapExprEntry::explicit(key, value))
                })
                .collect();
            ctx.alloc_expr(Expr::Map { entries }, span)
        }
        AnyExpr::Backtick(id) => templates::backtick_expr(ctx, v, id),
        AnyExpr::TaggedTemplate(id) => templates::tagged_template_expr(ctx, v, id),
        AnyExpr::Bogus(_) => ctx.alloc_expr(Expr::Missing, span),
    }
}

/// `a.b.c`, or `true`/`false`/`null` as the literals they are. A path of
/// several segments records each segment's span.
fn path_expr(ctx: &mut LoweringContext, v: View<'_>, id: Id<PathExpr>) -> ExprId {
    let node = *v.node(id);
    let span = v.range_of(id);
    let mut segments = vec![(v.name(node.head), v.name_range(node.head))];
    for segment in v.tree.list(node.segments) {
        // A dangling `.` was reported; the path ends before it.
        let Ok(name) = segment.name else { break };
        segments.push((v.name(name), v.name_range(name)));
    }
    if segments.len() == 1 {
        match segments[0].0.as_str() {
            "true" => return ctx.alloc_expr(Expr::Literal(Literal::Bool(true)), span),
            "false" => return ctx.alloc_expr(Expr::Literal(Literal::Bool(false)), span),
            "null" => return ctx.alloc_expr(Expr::Null, span),
            _ => {}
        }
    }
    let names: Vec<Name> = segments.iter().map(|(name, _)| name.clone()).collect();
    let id = ctx.alloc_expr(Expr::Path(baml_base::Path::new(names)), span);
    if segments.len() > 1 {
        let spans = segments.iter().map(|(_, range)| *range).collect();
        ctx.source_map.path_segment_spans.insert(id, spans);
    }
    id
}

/// `base<T>` in value position: a specialized reference to a generic function.
/// Anything but a bare path base is reported; a parenthesized path still
/// specializes, so a later "must be specialized" error does not pile on.
fn generic_apply(
    ctx: &mut LoweringContext,
    v: View<'_>,
    base_node: AnyExpr,
    base: ExprId,
    args: Id<GenericArgs>,
    span: TextRange,
) -> ExprId {
    let type_args = generic_args(ctx, v, args);
    if type_args.is_empty() {
        return base;
    }
    let parenthesized_base = matches!(base_node, AnyExpr::Paren(_));
    let path_base = matches!(ctx.exprs[base], Expr::Path(_));
    if parenthesized_base || !path_base {
        ctx.diags
            .push(LoweringDiagnostic::TypeArgsOnNonPathBase { span });
        if !path_base {
            return base;
        }
    }
    ctx.alloc_expr(Expr::GenericApply { base, type_args }, span)
}

fn generic_args(ctx: &mut LoweringContext, v: View<'_>, args: Id<GenericArgs>) -> Vec<TypeExpr> {
    let items: Vec<_> = v.tree.list(v.node(args).args).to_vec();
    items
        .into_iter()
        .map(|item| types::lower_type(v, &mut ctx.diags, item.item))
        .collect()
}

/// A literal token in expression position.
fn literal_token(
    ctx: &mut LoweringContext,
    v: View<'_>,
    token: baml_syntax::token::TokenIdx,
) -> Expr {
    let text = v.text(token);
    let range = v.token_range(token);
    match v.tree.tokens().kind(token) {
        TokenKind::Integer => Expr::Literal(Literal::Int(crate::lower_int_literal(
            text,
            range,
            &mut ctx.diags,
        ))),
        TokenKind::Bigint => Expr::Literal(Literal::Bigint(crate::lower_bigint_literal(
            text,
            range,
            &mut ctx.diags,
        ))),
        TokenKind::Float => Expr::Literal(Literal::Float(num_lit::normalize_float_literal(text))),
        TokenKind::True => Expr::Literal(Literal::Bool(true)),
        TokenKind::False => Expr::Literal(Literal::Bool(false)),
        TokenKind::Null => Expr::Null,
        kind => unreachable!("a literal expression token is a literal, not {kind:?}"),
    }
}

/// `"…"`, `r"…"`, `b"…"`, each decoded as the rowan lowering decoded it.
fn string_literal(ctx: &mut LoweringContext, v: View<'_>, id: Id<StringLit>) -> ExprId {
    let token = v.node(id).token.idx;
    let text = v.text(token);
    let span = v.range_of(id);
    let expr = match v.tree.tokens().kind(token) {
        TokenKind::RawString | TokenKind::UnclosedRawString => Expr::Literal(Literal::String(
            strip_raw_string_delimiters(text)
                .unwrap_or_default()
                .to_string(),
        )),
        TokenKind::ByteString | TokenKind::UnclosedByteString => {
            let content = text
                .strip_prefix("b\"")
                .and_then(|s| s.strip_suffix('"'))
                .unwrap_or("");
            Expr::ByteStringLiteral(baml_base::escape::unescape_byte_string_literal(content))
        }
        // Removed syntax, reported by the parser: no value.
        TokenKind::LegacyHashString | TokenKind::UnclosedHashString => Expr::Missing,
        // `String` and `UnclosedString`, the rest of `StringToken`.
        _ => Expr::Literal(Literal::String(strip_string_delimiters(text))),
    };
    ctx.alloc_expr(expr, span)
}

fn call_expr(
    ctx: &mut LoweringContext,
    v: View<'_>,
    id: Id<baml_syntax::nodes::CallExpr>,
) -> ExprId {
    let node = *v.node(id);
    let span = v.range_of(id);
    // Type arguments on the callee are the call's: `foo<T>(x)`, and the
    // receiver's on a static method of a generic class, `Box<T>.from(j)`.
    let (callee_node, type_args) = match node.callee {
        AnyExpr::GenericApply(apply) => {
            let apply = *v.node(apply);
            (apply.base, generic_args(ctx, v, apply.args))
        }
        callee => {
            let receiver_args = receiver_generic_args(v, callee);
            let type_args = receiver_args.map_or_else(Vec::new, |args| generic_args(ctx, v, args));
            (callee, type_args)
        }
    };
    let callee = match node.callee {
        // The call owns the type args; the callee is the path alone.
        AnyExpr::GenericApply(_) => expression_in_chain(ctx, v, callee_node),
        callee => callee_without_receiver_args(ctx, v, callee),
    };
    let (args, label_spans) = call_args(ctx, v, node.args);
    let args = ctx.complete_client_resolve_call(callee, args, span);
    let id = ctx.alloc_expr(
        Expr::Call {
            callee,
            type_args,
            args,
        },
        span,
    );
    ctx.record_call_arg_label_spans(id, label_spans);
    if ctx.needs_chain_wrap.remove(&callee) {
        ctx.needs_chain_wrap.insert(id);
    }
    id
}

/// The `<…>` on the receiver of a member callee (`Box<T>.from(j)`): the base
/// of a member-access chain that is itself a generic application, not
/// reached through an `.as<…>`.
fn receiver_generic_args(v: View<'_>, callee: AnyExpr) -> Option<Id<GenericArgs>> {
    let base = match callee {
        AnyExpr::FieldAccess(id) => v.node(id).base,
        AnyExpr::OptionalFieldAccess(id) => v.node(id).base,
        _ => return None,
    };
    match base {
        AnyExpr::GenericApply(id) => Some(v.node(id).args),
        AnyExpr::Upcast(_) => None,
        base => receiver_generic_args(v, base),
    }
}

/// A member callee whose receiver's generic arguments the call consumed: the
/// receiver lowers as its bare base, not as a `GenericApply`.
fn callee_without_receiver_args(ctx: &mut LoweringContext, v: View<'_>, callee: AnyExpr) -> ExprId {
    let span = v.range_any(&callee);
    match callee {
        AnyExpr::FieldAccess(id) => {
            let node = *v.node(id);
            let base = callee_without_receiver_args(ctx, v, node.base);
            let member = node
                .name
                .ok()
                .map(|name| (v.name(name), v.name_range(name)));
            let id = ctx.alloc_expr(
                Expr::MemberAccess {
                    base,
                    member: member
                        .as_ref()
                        .map_or_else(|| Name::new("_"), |(n, _)| n.clone()),
                },
                span,
            );
            if let Some((_, range)) = member {
                ctx.source_map.member_access_member_spans.insert(id, range);
            }
            if ctx.needs_chain_wrap.remove(&base) {
                ctx.needs_chain_wrap.insert(id);
            }
            id
        }
        AnyExpr::OptionalFieldAccess(id) => {
            let node = *v.node(id);
            let base = callee_without_receiver_args(ctx, v, node.base);
            let member = node
                .name
                .ok()
                .map(|name| (v.name(name), v.name_range(name)));
            let id = ctx.alloc_expr(
                Expr::OptionalMemberAccess {
                    base,
                    member: member
                        .as_ref()
                        .map_or_else(|| Name::new("_"), |(n, _)| n.clone()),
                },
                span,
            );
            if let Some((_, range)) = member {
                ctx.source_map.member_access_member_spans.insert(id, range);
            }
            ctx.needs_chain_wrap.remove(&base);
            ctx.needs_chain_wrap.insert(id);
            id
        }
        AnyExpr::GenericApply(id) => expression_in_chain(ctx, v, v.node(id).base),
        other => expression_in_chain(ctx, v, other),
    }
}

/// `e catch (x) { arms } catch …`: each clause protects everything before it.
fn catch_expr(
    ctx: &mut LoweringContext,
    v: View<'_>,
    id: Id<baml_syntax::nodes::CatchExpr>,
) -> ExprId {
    let node = *v.node(id);
    let span = v.range_of(id);
    let mut base = expression(ctx, v, node.body);
    let clauses: Vec<_> = v.tree.list(node.clauses.as_list()).to_vec();
    for clause_id in clauses {
        let clause_node = *v.node(clause_id);
        let clause_span = v.range_of(clause_id);
        let kind = match v.tree.tokens().kind(clause_node.kw.idx) {
            TokenKind::Catch => CatchClauseKind::Catch,
            TokenKind::CatchAll => CatchClauseKind::CatchAll,
            TokenKind::CatchAllPanics => CatchClauseKind::CatchAllPanics,
            kind => unreachable!("a catch keyword, not {kind:?}"),
        };
        let binding = match clause_node.binding {
            Ok(name) => ctx.alloc_pattern(
                Pattern::Bind {
                    name: v.name(name),
                    subpat: None,
                },
                v.name_range(name),
            ),
            Err(_) => ctx.alloc_pattern(Pattern::Wildcard, clause_span),
        };
        let stack_trace_binding = clause_node
            .trace
            .and_then(|trace| trace.name.ok())
            .map(|name| {
                ctx.alloc_pattern(
                    Pattern::Bind {
                        name: v.name(name),
                        subpat: None,
                    },
                    v.name_range(name),
                )
            });
        let arms: Vec<_> = v.tree.list(clause_node.arms).to_vec();
        let arms = arms
            .into_iter()
            .map(|arm_id| {
                let arm = *v.node(arm_id);
                let arm_span = v.range_of(arm_id);
                let pattern = match arm.pattern {
                    Ok(pattern) => self::pattern(ctx, v, pattern),
                    Err(_) => ctx.alloc_pattern(Pattern::Wildcard, arm_span),
                };
                let body = expr_or_missing(ctx, v, arm.body, arm_span);
                ctx.alloc_catch_arm(CatchArm { pattern, body }, arm_span)
            })
            .collect();
        let clause = CatchClause {
            kind,
            binding,
            stack_trace_binding,
            arms,
        };
        base = ctx.alloc_expr(
            Expr::Catch { base, clause },
            TextRange::new(span.start(), clause_span.end()),
        );
    }
    base
}

/// `spawn name with m1, m2 { body }` is
/// `baml.spawn.__spawn(baml.spawn.Plan.new(body = () -> { body }, name = name).with(m1).with(m2))`.
fn spawn_expr(
    ctx: &mut LoweringContext,
    v: View<'_>,
    id: Id<baml_syntax::nodes::SpawnExpr>,
) -> ExprId {
    let node = *v.node(id);
    let span = v.range_of(id);
    let name = node.name.map(|name| expression(ctx, v, name));
    let mut with_exprs = Vec::new();
    if let Some(with) = node.with {
        let items: Vec<_> = v.tree.list(v.node(with).items).to_vec();
        for item in items {
            let item_span = v.range_any(&item.item);
            with_exprs.push((expression(ctx, v, item.item), item_span));
        }
    }
    // No block, no lambda to hang the spawn off: the expression is missing.
    let Ok(block) = node.body else {
        return ctx.alloc_expr(Expr::Missing, span);
    };
    let block_span = v.range_of(block);
    let saved_collector = ctx.testset_collector_var.take();
    let body = block_expr(ctx, v, block);
    ctx.testset_collector_var = saved_collector;
    let lambda = LambdaDef {
        kind: LambdaKind::Spawn,
        params: Vec::new(),
        defaults: FunctionDefaults::empty(),
        return_type: None,
        throws: None,
        body: Some(body),
        span: block_span,
    };
    let body = ctx.alloc_expr(Expr::Lambda(Box::new(lambda)), block_span);
    let plan_new = ctx.alloc_expr(
        Expr::Path(baml_base::compiler_path!(baml.spawn.Plan.new)),
        span,
    );
    let mut plan_args = vec![CallArg::named("body", body)];
    plan_args.extend(name.map(|name| CallArg::named("name", name)));
    let mut plan = ctx.alloc_expr(
        Expr::Call {
            callee: plan_new,
            type_args: Vec::new(),
            args: plan_args,
        },
        span,
    );
    for (modifier, modifier_span) in with_exprs {
        let with = ctx.alloc_expr(
            Expr::MemberAccess {
                base: plan,
                member: Name::new("with"),
            },
            modifier_span,
        );
        plan = ctx.alloc_expr(
            Expr::Call {
                callee: with,
                type_args: Vec::new(),
                args: vec![CallArg::positional(modifier)],
            },
            modifier_span,
        );
    }
    let launch = ctx.alloc_expr(
        Expr::Path(baml_base::compiler_path!(baml.spawn.__spawn)),
        span,
    );
    ctx.alloc_expr(
        Expr::Call {
            callee: launch,
            type_args: Vec::new(),
            args: vec![CallArg::positional(plan)],
        },
        span,
    )
}

// ── Lambdas and parameters ───────────────────────────────────────────────────

fn lambda_expr(ctx: &mut LoweringContext, v: View<'_>, id: Id<LambdaExpr>) -> ExprId {
    let node = *v.node(id);
    let span = v.range_of(id);
    let (params, defaults) = lower_params(
        v,
        node.params,
        "lambda",
        "lambda parameters",
        &mut ctx.diags,
        false,
        &mut ctx.env_var_refs,
    );
    let return_type = node.ret.map(|ty| types::lower_type(v, &mut ctx.diags, ty));
    let throws = node
        .throws
        .and_then(|clause| v.node(clause).ty.ok())
        .map(|ty| types::lower_type(v, &mut ctx.diags, ty));
    // The body lowers into this arena; a `test` inside it registers nowhere.
    let body = node.body.ok().map(|block| {
        let saved_collector = ctx.testset_collector_var.take();
        let body = block_expr(ctx, v, block);
        ctx.testset_collector_var = saved_collector;
        body
    });
    let lambda = LambdaDef {
        kind: LambdaKind::Anonymous,
        params,
        defaults,
        return_type,
        throws,
        body,
        span,
    };
    ctx.alloc_expr(Expr::Lambda(Box::new(lambda)), span)
}

/// A parameter list, with its defaults in their own arena. A parameter with
/// no name is reported and dropped; its default is still lowered, as the
/// rowan lowering did, so the defaults arena reads the same.
pub(crate) fn lower_params(
    v: View<'_>,
    params: Id<ParamList>,
    owner_name: &str,
    unsupported_default_context: &str,
    diags: &mut Vec<LoweringDiagnostic>,
    defaults_allowed: bool,
    env_var_refs: &mut Vec<EnvVarRef>,
) -> (Vec<Param>, FunctionDefaults) {
    let items: Vec<_> = v
        .tree
        .list(v.node(params).params)
        .iter()
        .map(|item| item.item)
        .collect();
    let mut lowered = Vec::new();
    let mut defaults: Vec<(usize, AnyExpr)> = Vec::new();
    for id in items {
        let param = *v.node(id);
        let default = param.default.and_then(|default| v.node(default).value.ok());
        let index = lowered.len();
        let name = match param.name {
            Ok(name) => Some(name),
            Err(_) => {
                diags.push(LoweringDiagnostic::MissingParamName {
                    function_name: owner_name.to_string(),
                    span: v.range_of(id),
                });
                None
            }
        };
        if let Some(default) = default {
            if !defaults_allowed {
                diags.push(LoweringDiagnostic::UnsupportedParameterDefault {
                    context: unsupported_default_context.to_string(),
                    span: v.range_any(&default),
                });
            }
            defaults.push((if name.is_some() { index } else { usize::MAX }, default));
        }
        let Some(name) = name else { continue };
        let param_name = v.name(name);
        let type_expr = param
            .ascription
            .and_then(|ascription| v.node(ascription).ty.ok())
            .map(|ty| {
                let te_span = v.range_any(&ty);
                let mut expr = types::lower_type(v, diags, ty);
                check_missing_type(
                    &expr,
                    format!("parameter `{param_name}` in `{owner_name}`"),
                    te_span,
                    diags,
                );
                check_wildcard_type(&mut expr, "a parameter type", te_span, diags);
                expr.with_span(te_span)
            });
        lowered.push(Param {
            name: param_name,
            type_expr,
            default: None,
            span: v.range_of(id),
            name_span: v.name_range(name),
        });
    }
    let mut ctx = LoweringContext::new();
    let default_ids: Vec<(usize, DefaultExprId)> = defaults
        .into_iter()
        .map(|(index, default)| (index, DefaultExprId::new(expression(&mut ctx, v, default))))
        .collect();
    let (exprs, source_map, ctx_diags, ctx_env_refs) = ctx.finish(None);
    diags.extend(ctx_diags);
    env_var_refs.extend(ctx_env_refs);
    for (index, default) in default_ids {
        if let Some(param) = lowered.get_mut(index) {
            param.default = Some(default);
        }
    }
    (lowered, FunctionDefaults { exprs, source_map })
}

// ── Object literals ──────────────────────────────────────────────────────────

fn object_literal(ctx: &mut LoweringContext, v: View<'_>, id: Id<ObjectLiteral>) -> ExprId {
    let node = *v.node(id);
    let span = v.range_of(id);
    let (path, args) = match node.ctor {
        AnyObjectCtor::Path(path) => (Some(path), None),
        AnyObjectCtor::Generic(apply) => {
            let apply = *v.node(apply);
            match apply.base {
                AnyExpr::Path(path) => (Some(path), Some(apply.args)),
                _ => (None, Some(apply.args)),
            }
        }
        AnyObjectCtor::Bogus(_) => (None, None),
    };
    let type_args = args.map_or_else(Vec::new, |args| generic_args(ctx, v, args));
    let Some(path) = path else {
        // A recovered shape with no constructor name; diagnostic only.
        ctx.diags
            .push(LoweringDiagnostic::MissingObjectConstructor { span });
        return ctx.alloc_expr(Expr::Missing, span);
    };
    let path = *v.node(path);
    let mut segments = vec![v.name(path.head)];
    for segment in v.tree.list(path.segments) {
        let Ok(name) = segment.name else { break };
        segments.push(v.name(name));
    }
    let type_name = baml_base::Path::new(segments);

    let mut fields = Vec::new();
    let mut field_name_spans = Vec::new();
    let mut spreads = Vec::new();
    let members: Vec<_> = v.tree.list(node.members).to_vec();
    for (position, member) in members.into_iter().enumerate() {
        match member {
            AnyObjectMember::Field(field) => {
                let field = *v.node(field);
                // Only a name keys a field of a constructed object (a string
                // or template key is a map's); the rowan lowering read none.
                let AnyObjectKey::Name(key) = field.key else {
                    continue;
                };
                let key_node = *v.node(key);
                let key_span = v.range_of(key);
                let key_text = types::dotted_text(v, key_node.head, key_node.segments);
                let value = match field.value {
                    Some(value) => match value.value {
                        Ok(expr) => Some((expression(ctx, v, expr), true)),
                        Err(_) => None,
                    },
                    // Shorthand `{ f }` reads the binding `f`.
                    None if key_node.segments.is_empty() => Some((
                        ctx.alloc_expr(
                            Expr::Path(baml_base::Path::bare(Name::new(&key_text))),
                            key_span,
                        ),
                        false,
                    )),
                    None => None,
                };
                let Some((value, explicit)) = value else {
                    continue;
                };
                let key = Name::new(key_text);
                field_name_spans.push((value, key_span));
                fields.push(if explicit {
                    ObjectExprField::explicit(key, value)
                } else {
                    ObjectExprField::shorthand(key, value)
                });
            }
            AnyObjectMember::Spread(spread) => {
                if let Ok(expr) = v.node(spread).value {
                    let expr = expression(ctx, v, expr);
                    spreads.push(SpreadField { expr, position });
                }
            }
            AnyObjectMember::Bogus(_) => {}
        }
    }
    let object = ctx.alloc_expr(
        Expr::Object {
            type_name,
            type_args,
            fields,
            spreads,
        },
        span,
    );
    for (value, key_span) in field_name_spans {
        ctx.source_map
            .object_field_name_spans
            .insert((object, value), key_span);
    }
    object
}

/// The arguments of a call, with the span of each label.
fn call_args(
    ctx: &mut LoweringContext,
    v: View<'_>,
    args: Id<ArgList>,
) -> (Vec<CallArg>, Vec<(ExprId, TextRange)>) {
    let items: Vec<_> = v.tree.list(v.node(args).args).to_vec();
    let mut lowered = Vec::with_capacity(items.len());
    for item in items {
        let arg = *v.node(item.item);
        // An argument with no value was reported; it is not one.
        let Ok(value) = arg.value else { continue };
        let label = arg
            .label
            .map(|label| (v.name(label.name), v.name_range(label.name)));
        let expr = expression(ctx, v, value);
        lowered.push((
            CallArg {
                label: label.as_ref().map(|(name, _)| name.clone()),
                expr,
            },
            label.map(|(_, span)| span),
        ));
    }
    LoweringContext::finalize_call_args(lowered)
}

fn binary_expr(
    ctx: &mut LoweringContext,
    v: View<'_>,
    id: Id<baml_syntax::nodes::BinaryExpr>,
) -> ExprId {
    let node = *v.node(id);
    let span = v.range_of(id);
    let lhs = expression(ctx, v, node.lhs);
    let op = match node.op.op() {
        ops::BinaryOp::Add => BinaryOp::Add,
        ops::BinaryOp::Sub => BinaryOp::Sub,
        ops::BinaryOp::Mul => BinaryOp::Mul,
        ops::BinaryOp::Div => BinaryOp::Div,
        ops::BinaryOp::Mod => BinaryOp::Mod,
        ops::BinaryOp::Eq => BinaryOp::Eq,
        ops::BinaryOp::Ne => BinaryOp::Ne,
        ops::BinaryOp::Lt => BinaryOp::Lt,
        ops::BinaryOp::Le => BinaryOp::Le,
        ops::BinaryOp::Gt => BinaryOp::Gt,
        ops::BinaryOp::Ge => BinaryOp::Ge,
        ops::BinaryOp::And => BinaryOp::And,
        ops::BinaryOp::Or => BinaryOp::Or,
        ops::BinaryOp::BitAnd => BinaryOp::BitAnd,
        ops::BinaryOp::BitOr => BinaryOp::BitOr,
        ops::BinaryOp::BitXor => BinaryOp::BitXor,
        ops::BinaryOp::Shl => BinaryOp::Shl,
        ops::BinaryOp::Shr => BinaryOp::Shr,
        ops::BinaryOp::NullCoalesce => BinaryOp::NullCoalesce,
        ops::BinaryOp::Instanceof => {
            let op_span = v
                .tree
                .tokens()
                .text_range(baml_syntax::tree::TokenRange::new(
                    node.op
                        .tokens()
                        .next()
                        .unwrap_or_else(|| unreachable!("an operator has a token")),
                    baml_syntax::token::TokenIdx(
                        node.op
                            .tokens()
                            .last()
                            .unwrap_or_else(|| unreachable!("an operator has a token"))
                            .0
                            + 1,
                    ),
                ));
            ctx.diags
                .push(LoweringDiagnostic::InstanceofRemoved { span: op_span });
            return ctx.alloc_expr(Expr::Missing, span);
        }
    };
    let rhs = expr_or_missing(ctx, v, node.rhs, span);
    ctx.alloc_expr(Expr::Binary { op, lhs, rhs }, span)
}

fn unary_expr(ctx: &mut LoweringContext, v: View<'_>, id: Id<UnaryExpr>) -> ExprId {
    let node = *v.node(id);
    let span = v.range_of(id);
    let op_kind = v.tree.tokens().kind(node.op.idx);
    let op = ops::UnaryOp::from_kind(op_kind)
        .unwrap_or_else(|| unreachable!("a unary operator token is one of the unary operators"));
    // An integer literal written directly under the operator.
    let direct_int_lit = match node.operand {
        Ok(AnyExpr::Literal(lit)) => {
            let token = v.node(lit).token.idx;
            (v.tree.tokens().kind(token) == TokenKind::Integer).then(|| v.text(token))
        }
        _ => None,
    };
    let expr = expr_or_missing(ctx, v, node.operand, span);
    match op {
        // `~x` is `-x - 1`, every node of it synthesized.
        ops::UnaryOp::BitNot => {
            let prev_synth = std::mem::replace(&mut ctx.synthesizing, true);
            let neg = ctx.alloc_expr(
                Expr::Unary {
                    op: UnaryOp::Neg,
                    expr,
                },
                span,
            );
            let one = ctx.alloc_expr(Expr::Literal(Literal::Int(1)), span);
            let result = ctx.alloc_expr(
                Expr::Binary {
                    op: BinaryOp::Sub,
                    lhs: neg,
                    rhs: one,
                },
                span,
            );
            ctx.synthesizing = prev_synth;
            result
        }
        ops::UnaryOp::Neg => {
            // `-<literal>` beyond the positive range is one negative literal
            // (`INT_MIN`), never a positive intermediate the VM rejects.
            if let Some(text) = direct_int_lit
                && let Ok(value) = baml_base::num_lit::parse_int_literal(text)
                && value > (i64::MAX >> 1)
            {
                return ctx.alloc_expr(Expr::Literal(Literal::Int(value.wrapping_neg())), span);
            }
            ctx.alloc_expr(
                Expr::Unary {
                    op: UnaryOp::Neg,
                    expr,
                },
                span,
            )
        }
        ops::UnaryOp::Not => ctx.alloc_expr(
            Expr::Unary {
                op: UnaryOp::Not,
                expr,
            },
            span,
        ),
        // `--x` is `-(-x)`.
        ops::UnaryOp::PreDec => {
            let inner = ctx.alloc_expr(
                Expr::Unary {
                    op: UnaryOp::Neg,
                    expr,
                },
                span,
            );
            ctx.alloc_expr(
                Expr::Unary {
                    op: UnaryOp::Neg,
                    expr: inner,
                },
                span,
            )
        }
        // `++x` has no meaning; the operand stands (as in the rowan lowering).
        ops::UnaryOp::PreInc => expr,
    }
}

fn if_expr(ctx: &mut LoweringContext, v: View<'_>, id: Id<IfExpr>) -> ExprId {
    let node = *v.node(id);
    let span = v.range_of(id);
    let condition = expr_or_missing(ctx, v, node.cond, span);
    let then_branch = block_or_missing(ctx, v, node.then_branch, span);
    let else_branch = node
        .else_branch
        .and_then(|branch| branch.body.ok())
        .map(|body| else_body(ctx, v, body));
    ctx.alloc_expr(
        Expr::If {
            condition,
            then_branch,
            else_branch,
        },
        span,
    )
}

fn else_body(ctx: &mut LoweringContext, v: View<'_>, body: AnyElse) -> ExprId {
    match body {
        AnyElse::Block(block) => block_expr(ctx, v, block),
        AnyElse::If(id) => if_expr(ctx, v, id),
        AnyElse::IfLet(id) => if_let_expr(ctx, v, id),
        AnyElse::Bogus(id) => ctx.alloc_expr(Expr::Missing, v.range_of(id)),
    }
}

fn if_let_expr(ctx: &mut LoweringContext, v: View<'_>, id: Id<IfLetExpr>) -> ExprId {
    let node = *v.node(id);
    let span = v.range_of(id);
    let pattern = match node.pattern {
        Ok(pattern) => self::pattern(ctx, v, pattern),
        Err(_) => ctx.alloc_pattern(Pattern::Wildcard, span),
    };
    let scrutinee = expr_or_missing(ctx, v, node.scrutinee, span);
    let then_branch = block_or_missing(ctx, v, node.then_branch, span);
    let else_branch = node
        .else_branch
        .and_then(|branch| branch.body.ok())
        .map(|body| else_body(ctx, v, body));
    ctx.alloc_expr(
        Expr::IfLet {
            pattern,
            scrutinee,
            then_branch,
            else_branch,
        },
        span,
    )
}

fn match_expr(
    ctx: &mut LoweringContext,
    v: View<'_>,
    id: Id<baml_syntax::nodes::MatchExpr>,
) -> ExprId {
    let node = *v.node(id);
    let span = v.range_of(id);
    let (scrutinee, scrutinee_type) = match node.scrutinee {
        Ok(AnyMatchScrutinee::Bare(expr)) => (Some(expression(ctx, v, expr)), None),
        Ok(AnyMatchScrutinee::Paren(paren)) => {
            let paren = *v.node(paren);
            let scrutinee = paren.expr.ok().map(|expr| expression(ctx, v, expr));
            let scrutinee_type = paren
                .ascription
                .and_then(|ascription| v.node(ascription).ty.ok())
                .map(|ty| {
                    let ty_span = v.range_any(&ty);
                    let ty = types::lower_type(v, &mut ctx.diags, ty);
                    ctx.alloc_type_annot(ty, ty_span)
                });
            (scrutinee, scrutinee_type)
        }
        Ok(AnyMatchScrutinee::Bogus(_)) | Err(_) => (None, None),
    };
    let arms: Vec<_> = v.tree.list(node.arms).to_vec();
    let arms = arms
        .into_iter()
        .map(|arm_id| {
            let arm = *v.node(arm_id);
            let arm_span = v.range_of(arm_id);
            let pattern = arm
                .pattern
                .ok()
                .map(|pattern| self::pattern(ctx, v, pattern));
            let guard = arm
                .guard
                .and_then(|guard| guard.cond.ok())
                .map(|cond| expression(ctx, v, cond));
            let body = arm.body.ok().map(|body| expression(ctx, v, body));
            let arm = MatchArm {
                pattern: pattern.unwrap_or_else(|| ctx.alloc_pattern(Pattern::Wildcard, arm_span)),
                guard,
                body: body.unwrap_or_else(|| ctx.alloc_expr(Expr::Missing, arm_span)),
            };
            ctx.alloc_match_arm(arm, arm_span)
        })
        .collect();
    // A match with no scrutinee has it nowhere (as in the rowan lowering).
    let scrutinee =
        scrutinee.unwrap_or_else(|| ctx.alloc_expr(Expr::Missing, TextRange::default()));
    ctx.alloc_expr(
        Expr::Match {
            scrutinee,
            scrutinee_type,
            arms,
        },
        span,
    )
}

// ── Patterns ─────────────────────────────────────────────────────────────────

pub(super) fn pattern(ctx: &mut LoweringContext, v: View<'_>, pattern: AnyPattern) -> PatId {
    let span = v.range_any(&pattern);
    match pattern {
        AnyPattern::Binding(id) => binding_pattern(ctx, v, id),
        AnyPattern::Wildcard(id) => {
            let node = *v.node(id);
            if let Some(introducer) = node.introducer {
                warn_const_introducer(ctx, v, introducer);
            }
            ctx.alloc_pattern(Pattern::Wildcard, span)
        }
        AnyPattern::Destructure(id) => destructure_pattern(ctx, v, id),
        AnyPattern::Array(id) => array_pattern(ctx, v, id),
        AnyPattern::Type(id) => {
            let ty = types::lower_type(v, &mut ctx.diags, v.node(id).ty);
            ctx.alloc_pattern(Pattern::Type(ty), span)
        }
        AnyPattern::Paren(id) => match v.node(id).inner {
            Ok(inner) => self::pattern(ctx, v, inner),
            Err(_) => ctx.alloc_pattern(Pattern::Wildcard, span),
        },
        AnyPattern::Union(id) => {
            let node = *v.node(id);
            let mut parts = vec![self::pattern(ctx, v, node.first)];
            let arms: Vec<_> = v.tree.list(node.arms.as_list()).to_vec();
            for arm in arms {
                // A `|` with nothing after it was reported; it is no alternative.
                if let Ok(pattern) = arm.pattern {
                    parts.push(self::pattern(ctx, v, pattern));
                }
            }
            match parts.len() {
                1 => parts[0],
                _ => ctx.alloc_pattern(Pattern::Or(parts), span),
            }
        }
        AnyPattern::Bogus(_) => ctx.alloc_pattern(Pattern::Wildcard, span),
    }
}

fn binding_pattern(ctx: &mut LoweringContext, v: View<'_>, id: Id<BindingPattern>) -> PatId {
    let node = *v.node(id);
    let span = v.range_of(id);
    if let Some(introducer) = node.introducer {
        warn_const_introducer(ctx, v, introducer);
    }
    let name = v.name(node.name);
    let name_span = v.name_range(node.name);
    if name.as_str() == "const" {
        ctx.diags
            .push(LoweringDiagnostic::ReservedConstBindingName { span: name_span });
    }
    let subpat = node
        .ascription
        .and_then(|ascription| ascription.pattern.ok())
        .map(|sub| pattern(ctx, v, sub));
    let id = ctx.alloc_pattern(Pattern::Bind { name, subpat }, span);
    ctx.source_map.bind_name_spans.insert(id, name_span);
    id
}

fn destructure_pattern(
    ctx: &mut LoweringContext,
    v: View<'_>,
    id: Id<DestructurePattern>,
) -> PatId {
    let node = *v.node(id);
    let span = v.range_of(id);
    if let Some(introducer) = node.introducer {
        warn_const_introducer(ctx, v, introducer);
    }
    let path = *v.node(node.path);
    let mut class = vec![v.name(path.head)];
    for segment in v.tree.list(path.segments) {
        let Ok(name) = segment.name else { break };
        class.push(v.name(name));
    }
    let mut generic_args = Vec::new();
    let mut associated_type_bindings = Vec::new();
    if let Some(args) = path.args {
        let items: Vec<_> = v.tree.list(v.node(args).args).to_vec();
        for item in items {
            match item.item {
                AnyTypeArg::Type(ty) => {
                    generic_args.push(types::lower_type(v, &mut ctx.diags, ty));
                }
                AnyTypeArg::Binding(binding) => {
                    let binding = *v.node(binding);
                    let ty = types::lower_type_or_missing(v, &mut ctx.diags, binding.ty);
                    associated_type_bindings.push(crate::ast::AssociatedTypeBinding {
                        name: v.name(binding.name),
                        ty: Box::new(ty),
                    });
                }
                AnyTypeArg::Bogus(_) => {}
            }
        }
    }
    let fields: Vec<_> = v
        .tree
        .list(node.fields)
        .iter()
        .map(|item| item.item)
        .collect();
    let fields = fields
        .into_iter()
        .map(|field| {
            let field = *v.node(field);
            let field_name = v.name(field.name);
            let field_span = v.name_range(field.name);
            let pat = match field.sub.and_then(|sub| sub.pattern.ok()) {
                Some(sub) => pattern(ctx, v, sub),
                // Shorthand `{ f }` binds `f`.
                None => ctx.alloc_pattern(
                    Pattern::Bind {
                        name: field_name.clone(),
                        subpat: None,
                    },
                    v.range_of(field.name),
                ),
            };
            FieldPat {
                field: field_name,
                field_span,
                pat,
            }
        })
        .collect();
    ctx.alloc_pattern(
        Pattern::Class {
            class,
            generic_args,
            associated_type_bindings,
            fields,
        },
        span,
    )
}

fn array_pattern(ctx: &mut LoweringContext, v: View<'_>, id: Id<ArrayPattern>) -> PatId {
    let node = *v.node(id);
    let span = v.range_of(id);
    let mut prefix = Vec::new();
    let mut rest = None;
    let mut suffix = Vec::new();
    let elements: Vec<_> = v
        .tree
        .list(node.elements)
        .iter()
        .map(|item| item.item)
        .collect();
    for element in elements {
        match element {
            AnyArrayPatternElement::Pattern(pat) => {
                let pat = pattern(ctx, v, pat);
                if rest.is_some() {
                    suffix.push(pat);
                } else {
                    prefix.push(pat);
                }
            }
            AnyArrayPatternElement::Rest(id) => {
                let pat = v.node(id).pattern.map(|pat| pattern(ctx, v, pat));
                if rest.is_none() {
                    rest = Some(ArrayRestPat { pat });
                }
            }
            AnyArrayPatternElement::Bogus(_) => {}
        }
    }
    let ascription = node
        .ascription
        .and_then(|ascription| v.node(ascription).ty.ok())
        .map(|ty| types::lower_type(v, &mut ctx.diags, ty));
    ctx.alloc_pattern(
        Pattern::Array {
            prefix,
            rest,
            suffix,
            ascription,
        },
        span,
    )
}

impl LoweringContext {
    /// `break`/`continue` in expression position: a block holding the jump.
    pub(crate) fn lower_jump_expr_at(&mut self, jump: Stmt, span: TextRange) -> ExprId {
        let stmt = self.alloc_stmt(jump, span);
        self.alloc_expr(
            Expr::Block {
                stmts: vec![stmt],
                tail_expr: None,
            },
            span,
        )
    }
}
