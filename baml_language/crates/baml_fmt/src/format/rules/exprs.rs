//! Expressions other than control flow: atoms, operators, parentheses,
//! literals, calls and member chains.
//!
//! Parentheses print as written except where they are redundant and carry
//! no comment: around the receiver of a member access, call or index when
//! the inside binds at least as tightly (`(xs).join()` → `xs.join()`);
//! around a unary operand that does (`-((1))` → `-1`); around a call
//! argument (`f((x))` → `f(x)`); and around the left operand of an operator
//! in the same chaining group (`(a + b) - c` → `a + b - c`). Parentheses
//! that clarify precedence (`(a * b) + c`, `a - (b - c)`, `(a ?? b).len()`)
//! stay, and so do the ones a call needs around a member access it calls
//! the value of (`(v.f)(3)`, `(t.0)(x)`). A one-element tuple `(x,)` is not
//! a parenthesised expression and keeps its comma.
//!
//! A chain of binary operators of one precedence row (`+ -`, `* / %`, `??`,
//! …) breaks one operand per line with the operator leading, indented one
//! level; the first operand stays on the line, and an operator after an
//! operand that broke inside braces of its own stays on its `}`. Operators
//! of different rows nest. A member chain breaks in the order: all on one
//! line — within rustfmt's chain width when it has two or more calls;
//! the last call's last argument hugged (a lambda, `spawn`, object, array or
//! map, or a sole nested call); every call on its own line under the
//! receiver, plain accesses glued to the call before them, when each of
//! those lines then fits whole; only the last call's arguments broken; else
//! every call on its own line with the arguments breaking as they must.
//! Every layout shares one printing of the arguments, so nesting costs no
//! more than its size.

use baml_syntax::{
    Token,
    nodes::{
        AnyExpr, AnyId, AnyObjectKey, AnyObjectMember, Arg, ArgList, ArrayLiteral, AwaitExpr,
        BinaryExpr, BreakExpr, CallExpr, ContinueExpr, EnvAccessExpr, FieldAccessExpr, FileTree,
        GenericApplyExpr, IndexExpr, IsExpr, LiteralExpr, MapEntry, MapLiteral, ObjectField,
        ObjectLiteral, OptionalCallExpr, OptionalFieldAccessExpr, OptionalIndexExpr, ParenExpr,
        PathExpr, ReturnExpr, SpecExpr, SpreadElement, StringLit, TaggedTemplateExpr, ThrowExpr,
        TupleExpr, TupleFieldExpr, UnaryExpr, UpcastExpr,
    },
    ops::{BinaryOp, BinaryOpTok},
    token::{Token, TokenIdx, TokenKind, punct},
    tree::{AnyNodeId, Id, NodeUnion as _, SepItem},
};

use super::control::{IfHead, if_chain, lambda, spawn};
use crate::{
    comments::{CommentKind, SourceComment, docs},
    format::{
        FormatNode, Formatter,
        layout::{ListItem, braced, multi_line_before_last},
    },
    ir::{Doc, Docs},
};

// ---------------------------------------------------------------------------
// Parentheses
// ---------------------------------------------------------------------------

/// The expression inside `paren` when the parentheses carry no comment, so
/// that dropping them loses nothing.
fn transparent(f: &Formatter<'_>, paren: Id<ParenExpr>) -> Option<AnyExpr> {
    let inner = f.tree.present(f.tree.get(paren).inner);
    let clean = f.comments.of(AnyNodeId::of(paren)).is_empty()
        && f.comments.leading(inner.node_id()).is_empty()
        && f.comments.trailing(inner.node_id()).is_empty();
    clean.then_some(inner)
}

/// `expr` without its transparent parentheses, however many, when what is
/// inside them all can stand there bare; else `expr` as it is (a pair that
/// stays collapses the ones inside it when it prints).
fn peel(f: &Formatter<'_>, expr: AnyExpr, can_drop: impl Fn(AnyExpr) -> bool) -> AnyExpr {
    let mut inner = expr;
    while let AnyExpr::Paren(paren) = inner {
        match transparent(f, paren) {
            Some(deeper) => inner = deeper,
            None => break,
        }
    }
    if !matches!(inner, AnyExpr::Paren(_)) && can_drop(inner) {
        inner
    } else {
        expr
    }
}

/// Whether `expr` can be the receiver of a member access, call or index
/// without parentheses: a name, a literal that brackets itself, or a
/// postfix chain on one of those that no `?.` is on the spine of (see
/// [`ends_optional_chain`]) — a chain the parser hung on something else
/// (`x is Foo.m()` reads the call on the `is`) binds no tighter than that
/// something.
fn binds_as_postfix_operand(f: &Formatter<'_>, expr: AnyExpr) -> bool {
    match expr {
        AnyExpr::Call(_)
        | AnyExpr::Index(_)
        | AnyExpr::FieldAccess(_)
        | AnyExpr::TupleField(_)
        | AnyExpr::GenericApply(_)
        | AnyExpr::Upcast(_)
        | AnyExpr::Spec(_)
        | AnyExpr::TaggedTemplate(_) => {
            !ends_optional_chain(f.tree, expr) && is_atom(spine_base(f.tree, expr))
        }
        other => is_atom(other),
    }
}

/// Whether an expression stands on its own as a receiver: a name or a
/// literal with brackets of its own.
fn is_atom(expr: AnyExpr) -> bool {
    matches!(
        expr,
        AnyExpr::Path(_)
            | AnyExpr::EnvAccess(_)
            | AnyExpr::Array(_)
            | AnyExpr::String(_)
            | AnyExpr::Backtick(_)
    )
}

/// What a postfix expression applies to: the receiver of a member access,
/// call, index, generic application, upcast, `@spec` or tagged template.
fn postfix_base(tree: &FileTree, expr: AnyExpr) -> Option<AnyExpr> {
    Some(match expr {
        AnyExpr::Call(call) => tree.get(call).callee,
        AnyExpr::OptionalCall(call) => tree.get(call).callee,
        AnyExpr::Index(index) => tree.get(index).base,
        AnyExpr::OptionalIndex(index) => tree.get(index).base,
        AnyExpr::FieldAccess(access) => tree.get(access).base,
        AnyExpr::OptionalFieldAccess(access) => tree.get(access).base,
        AnyExpr::TupleField(access) => tree.get(access).base,
        AnyExpr::GenericApply(apply) => tree.get(apply).base,
        AnyExpr::Upcast(upcast) => tree.get(upcast).base,
        AnyExpr::Spec(spec) => tree.get(spec).base,
        AnyExpr::TaggedTemplate(tagged) => tree.get(tagged).tag,
        _ => return None,
    })
}

/// The expression at the bottom of `expr`'s postfix spine, parentheses on
/// the way read through: what the whole chain is applied to.
fn spine_base(tree: &FileTree, mut expr: AnyExpr) -> AnyExpr {
    loop {
        expr = match expr {
            AnyExpr::Paren(paren) => match tree.get(paren).inner {
                Ok(inner) => inner,
                Err(_) => return expr,
            },
            other => match postfix_base(tree, other) {
                Some(base) => base,
                None => return other,
            },
        };
    }
}

/// Whether an optional access (`?.`) is on the postfix spine of `expr`,
/// parentheses on the way read through: parentheses around it end the
/// short-circuit there (`(a?.b).c` reads `.c` on `null` where `a?.b.c`
/// skips it), so they mean something and stay. The oracle keeps them for the
/// same reason, by this same test.
pub(crate) fn ends_optional_chain(tree: &FileTree, mut expr: AnyExpr) -> bool {
    loop {
        expr = match expr {
            AnyExpr::OptionalFieldAccess(_)
            | AnyExpr::OptionalIndex(_)
            | AnyExpr::OptionalCall(_) => {
                return true;
            }
            AnyExpr::TupleField(access)
                if tree.tokens().kind(tree.get(access).dot.idx) == TokenKind::QuestionDot =>
            {
                return true;
            }
            AnyExpr::Paren(paren) => match tree.get(paren).inner {
                Ok(inner) => inner,
                Err(_) => return false,
            },
            other => match postfix_base(tree, other) {
                Some(base) => base,
                None => return false,
            },
        };
    }
}

/// Whether `expr` is the name `env` in parentheses (however many): a field
/// access on it must keep them, since `env.NAME` is an environment access.
fn is_bare_env(f: &Formatter<'_>, expr: AnyExpr) -> bool {
    matches!(expr, AnyExpr::Paren(_))
        && matches!(
            peel(f, expr, |_| true),
            AnyExpr::Path(path)
                if f.tree.list(f.tree.get(path).segments).is_empty()
                    && f.node_text(AnyNodeId::of(f.tree.get(path).head)) == "env"
        )
}

/// A receiver: parentheses around something that binds tightly enough go.
fn postfix_receiver(f: &Formatter<'_>, expr: AnyExpr) -> AnyExpr {
    peel(f, expr, |inner| binds_as_postfix_operand(f, inner))
}

/// A prefix operator's operand, with the comments of what was peeled off it.
struct Operand {
    expr: AnyExpr,
    /// The leading comments of the dropped parentheses, outermost first,
    /// then the operand's own.
    leading: Vec<SourceComment>,
    /// The operand's own trailing comments, then the dropped parentheses',
    /// innermost first.
    trailing: Vec<SourceComment>,
}

/// A prefix operator's operand without its redundant parentheses — as a
/// receiver needs none, and a literal needs none either. The parentheses
/// stay when a comment dangles inside them, when a comment on a line of its
/// own precedes the `)`, or when one on a line of its own (or a line
/// comment) leads what is inside: without them that comment would attach to
/// whatever ends where the operand ends, or sit before the operator at a
/// level that depends on what encloses the expression. A comment on the
/// operand's own line inside them (`-(/* c */ x)`) is carried out in front of
/// the operator.
fn unary_operand(f: &Formatter<'_>, expr: AnyExpr) -> Operand {
    let own_line_trailing = |id: AnyNodeId| {
        f.comments
            .trailing(id)
            .iter()
            .any(|comment| comment.before.is_line_break())
    };
    let line_breaking_leading = |id: AnyNodeId| {
        f.comments
            .leading(id)
            .iter()
            .any(|comment| comment.kind == CommentKind::Line || comment.before.is_line_break())
    };
    let mut parens = Vec::new();
    let mut inner = expr;
    while let AnyExpr::Paren(paren) = inner {
        let next = f.tree.present(f.tree.get(paren).inner);
        let anchors = !f.comments.dangling(AnyNodeId::of(paren)).is_empty()
            || own_line_trailing(AnyNodeId::of(paren))
            || own_line_trailing(next.node_id())
            || line_breaking_leading(next.node_id());
        if anchors {
            break;
        }
        parens.push(AnyNodeId::of(paren));
        inner = next;
    }
    let bare = !matches!(inner, AnyExpr::Paren(_))
        && (binds_as_postfix_operand(f, inner) || matches!(inner, AnyExpr::Literal(_)));
    if !bare {
        parens.clear();
        inner = expr;
    }
    let leading = parens
        .iter()
        .copied()
        .chain([inner.node_id()])
        .flat_map(|id| f.comments.leading(id))
        .copied()
        .collect();
    let trailing = [inner.node_id()]
        .into_iter()
        .chain(parens.iter().rev().copied())
        .flat_map(|id| f.comments.trailing(id))
        .copied()
        .collect();
    Operand {
        expr: inner,
        leading,
        trailing,
    }
}

/// A call argument: no parentheses are needed, except that a lambda or
/// `spawn` keeps the ones it was written with.
fn argument(f: &Formatter<'_>, expr: AnyExpr) -> AnyExpr {
    let peeled = peel(f, expr, |_| true);
    if matches!(peeled, AnyExpr::Lambda(_) | AnyExpr::Spawn(_)) {
        expr
    } else {
        peeled
    }
}

/// `(inner)`, the inside on a line of its own when it does not fit; nested
/// transparent parentheses collapse into this pair. An inside that ends in
/// brackets of its own (an `if`, a `match`, a `catch`, a literal — see
/// `block_like` — or an operator chain ending in one) hugs the
/// parentheses and breaks inside those brackets, as rustfmt keeps
/// `(match x {` … `}).y`.
impl FormatNode for ParenExpr {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        let d = f.docs;
        let inner = peel(f, f.tree.present(node.inner), |_| true);
        let ends_in_braces = match inner {
            AnyExpr::Binary(binary) => braced(rightmost_operand(f, binary).node_id()),
            other => block_like(other.node_id()),
        };
        if ends_in_braces {
            return d.concat([
                Doc::LINE_SUFFIX_BOUNDARY,
                d.text("("),
                f.comments_inside(node.l.idx),
                f.space_after_opener(node.l.idx),
                f.alt(inner),
                f.token(f.tree.present(node.r)),
            ]);
        }
        d.group(d.concat([
            Doc::LINE_SUFFIX_BOUNDARY,
            d.text("("),
            f.comments_inside(node.l.idx),
            d.indent(d.concat([f.line_after_opener(node.l.idx), f.alt(inner)])),
            Doc::SOFT_LINE,
            f.token(f.tree.present(node.r)),
        ]))
    }
}

// ---------------------------------------------------------------------------
// Atoms
// ---------------------------------------------------------------------------

impl FormatNode for LiteralExpr {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        f.token(f.tree.get(id).token)
    }
}

impl FormatNode for StringLit {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        f.token(f.tree.get(id).token)
    }
}

/// `env.NAME`
impl FormatNode for EnvAccessExpr {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        f.docs.concat([
            f.token(node.env),
            f.token(node.dot),
            f.node(f.tree.present(node.name)),
        ])
    }
}

// ---------------------------------------------------------------------------
// Operators
// ---------------------------------------------------------------------------

/// The operators that chain, by precedence row: a run of operators of one
/// row lays out as one chain, however the source nested it, and the
/// parentheses around a left operand in the same row go, since they change
/// nothing (`(a + b) - c`). Operators of different rows nest, as rustfmt
/// nests them: `a || b && c` keeps `b && c` together on its line. The
/// comparisons do not chain — `a == b == c` is not written to be read as a
/// run — and neither do `instanceof` and the parentheses that keep an `<`
/// from reading as a generic.
fn chain_row(op: BinaryOp) -> Option<u8> {
    match op {
        BinaryOp::Add | BinaryOp::Sub => Some(0),
        BinaryOp::Mul | BinaryOp::Div | BinaryOp::Mod => Some(1),
        BinaryOp::BitAnd => Some(2),
        BinaryOp::BitOr => Some(3),
        BinaryOp::BitXor => Some(4),
        BinaryOp::And => Some(5),
        BinaryOp::Or => Some(6),
        BinaryOp::NullCoalesce => Some(7),
        BinaryOp::Shl | BinaryOp::Shr => Some(8),
        BinaryOp::Eq
        | BinaryOp::Ne
        | BinaryOp::Lt
        | BinaryOp::Gt
        | BinaryOp::Le
        | BinaryOp::Ge
        | BinaryOp::Instanceof => None,
    }
}

/// One `op rhs` of a chain, with the comments of a flattened operand around
/// it: those before a right operand that was itself a binary expression,
/// and those after it.
struct Member<'a> {
    before: &'a [SourceComment],
    op: BinaryOpTok,
    rhs: AnyExpr,
    after: Doc<'a>,
}

/// The chain `id` heads: its first operand and the members after it, nested
/// binary expressions in the same row lifted out, transparent parentheses
/// around a left operand in the same row peeled. The left spine — where a
/// long chain nests, one level per operator — is walked with a loop, so a
/// generated chain of any length costs no stack; a nested chain on the
/// right, which only a right-nested parse produces (the chaining operators
/// all nest left), is walked by recursion.
fn chain_members<'a>(
    f: &Formatter<'a>,
    id: Id<BinaryExpr>,
    members: &mut Vec<Member<'a>>,
) -> AnyExpr {
    let tree = f.tree;
    let Some(row) = chain_row(tree.get(id).op.op()) else {
        let node = tree.get(id);
        members.push(Member {
            before: &[],
            op: node.op,
            rhs: tree.present(node.rhs),
            after: Doc::NIL,
        });
        return node.lhs;
    };
    // A nested chain in the same row flattens into this one — unless its last
    // operand would then take the operator after it as part of itself
    // (`(1 + return 2) + 3` is not `1 + return 2 + 3`), or ends in braces
    // of its own, which the author's parentheses keep apart from what
    // follows (`(a + if (c) { 1 } else { 2 }) + 3`).
    let op = tree.get(id).op.op();
    let in_group = |expr: AnyExpr| match expr {
        AnyExpr::Binary(inner) => {
            let last = rightmost_operand(f, inner);
            chain_row(tree.get(inner).op.op()) == Some(row)
                && !takes_the_operator(f, last, op)
                && !braced(last.node_id())
        }
        _ => false,
    };
    // The spine, outermost first, down to the node whose left operand is
    // not a chain in this row; that operand is the chain's first.
    let mut spine = vec![id];
    let first = loop {
        let at = *spine
            .last()
            .unwrap_or_else(|| unreachable!("the spine starts with `id`"));
        let lhs = peel(f, tree.get(at).lhs, in_group);
        match lhs {
            AnyExpr::Binary(left) if in_group(lhs) => spine.push(left),
            other => break other,
        }
    };
    for (depth, &at) in spine.iter().enumerate().rev() {
        let node = tree.get(at);
        // A left operand that was a chain of its own ends with the member
        // before this node's: its trailing comments go after that member.
        if let Some(&left) = spine.get(depth + 1) {
            let trailing = docs::trailing(f.docs, tree, f.comments.trailing(AnyNodeId::of(left)));
            if let Some(last) = members.last_mut() {
                last.after = f.docs.concat([last.after, trailing]);
            }
        }
        let rhs = tree.present(node.rhs);
        match rhs {
            AnyExpr::Binary(right) if in_group(rhs) => {
                let before = f.comments.leading(AnyNodeId::of(right));
                let start = members.len();
                let right_first = chain_members(f, right, members);
                members.insert(
                    start,
                    Member {
                        before,
                        op: node.op,
                        rhs: right_first,
                        after: Doc::NIL,
                    },
                );
                let trailing =
                    docs::trailing(f.docs, tree, f.comments.trailing(AnyNodeId::of(right)));
                if let Some(last) = members.last_mut() {
                    last.after = f.docs.concat([last.after, trailing]);
                }
            }
            rhs => members.push(Member {
                before: &[],
                op: node.op,
                rhs,
                after: Doc::NIL,
            }),
        }
    }
    first
}

/// The last operand of the chain `id` heads, through transparent
/// parentheses and nested binary expressions on the right.
fn rightmost_operand(f: &Formatter<'_>, id: Id<BinaryExpr>) -> AnyExpr {
    let mut expr = f.tree.present(f.tree.get(id).rhs);
    loop {
        expr = peel(f, expr, |_| true);
        match expr {
            AnyExpr::Binary(inner) => expr = f.tree.present(f.tree.get(inner).rhs),
            other => return other,
        }
    }
}

/// Whether an expression, written bare before `op`, would take the
/// operator and what follows it as its own: a `return` or `throw` with a
/// payload takes the rest of the expression, whatever operator follows; an
/// `is` pattern takes a `|` as another alternative and a `??` as two `?`
/// suffixes on its type. A prefix operator or an `await` binds above every
/// binary one ([`PREFIX_BINDING_POWER`](baml_syntax::ops::PREFIX_BINDING_POWER))
/// and so never takes the operator itself, but its operand decides:
/// `-return b + c` is `-(return (b + c))`.
fn takes_the_operator(f: &Formatter<'_>, expr: AnyExpr, op: BinaryOp) -> bool {
    match expr {
        AnyExpr::Return(ret) => f.tree.get(ret).value.is_some(),
        AnyExpr::Throw(_) => true,
        AnyExpr::Unary(unary) => {
            takes_the_operator(f, f.tree.present(f.tree.get(unary).operand), op)
        }
        AnyExpr::Await(awaited) => {
            takes_the_operator(f, f.tree.present(f.tree.get(awaited).operand), op)
        }
        AnyExpr::Is(_) => matches!(op, BinaryOp::BitOr | BinaryOp::NullCoalesce),
        _ => false,
    }
}

/// `a + b - c`: the first operand on the line, each further `op operand`
/// after it, all on one line or each on a line of its own, indented. The
/// first operand's trailing comments go inside that indentation, with the
/// operators they precede. The comments between an operator and its operand
/// stay where they were written, in order: a line comment ends the
/// operator's line and the operand follows one level in — unless one of
/// them is on a line of its own, when they all go before the operator, on
/// lines of their own in the order written, so that the operator never
/// ends its line: `x -` followed by a line reads as `x` and then a
/// negation. An operator after
/// an operand in braces of its own (`if (c) { … } else { … } - x`) stays on
/// its closing line, as rustfmt keeps it, and its operand is laid out as
/// part of that line: the braces break inside themselves instead; an
/// operand in braces of its own stays on its operator's line likewise,
/// while `+ match (x) {` fits there. After any
/// other operand that broke and left only closing brackets on its last
/// line, the operator joins that line when its operand fits there —
/// rustfmt's orphan rule, `}) ?? return null` (see [`Docs::snuggle`]).
impl FormatNode for BinaryExpr {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let d = f.docs;
        let mut members = Vec::new();
        let first = chain_members(f, id, &mut members);
        let first_id = first.node_id();
        let first_doc = f.bare(first_id);
        // The first operand's trailing comments go with the operators they
        // precede: inside the indentation, unless the operand is braced and
        // the operator stays at its level.
        let first_trailing = f.trailing_of(first_id);
        let mut rest = vec![if braced(first_id) {
            first_trailing
        } else {
            d.indent(first_trailing)
        }];
        let mut operands = vec![first_doc];
        let mut previous = first_id;
        for member in members {
            let rhs_id = member.rhs.node_id();
            let between: Vec<SourceComment> = member
                .op
                .tokens()
                .flat_map(|token| f.dangling_after(token))
                .chain(member.before)
                .chain(f.comments.leading(rhs_id))
                .copied()
                .collect();
            let rhs = d.concat([f.bare(rhs_id), f.trailing_of(rhs_id)]);
            let (hoisted, link) = if between.iter().any(|comment| comment.before.is_line_break()) {
                // Each on a line of its own before the operator; a run on one
                // line in the source stays one line.
                let hoisted = d.concat(between.iter().enumerate().map(|(i, comment)| {
                    let line = if i == 0 || comment.before.is_line_break() {
                        Doc::HARD_LINE
                    } else {
                        Doc::SPACE
                    };
                    d.concat([line, d.text(comment.text(f.tree))])
                }));
                let bare_operator =
                    d.concat([Doc::LINE_SUFFIX_BOUNDARY, d.text(member.op.op().spelling())]);
                (
                    hoisted,
                    d.concat([bare_operator, Doc::SPACE, rhs, member.after]),
                )
            } else {
                // A line comment among them ends the operator's line, and
                // the operand follows one level in, as after `let x = // c`.
                let leading = docs::leading(d, f.tree, &between);
                let ends_line = between
                    .iter()
                    .any(|comment| comment.kind == CommentKind::Line);
                let operand = d.concat([Doc::SPACE, leading, rhs]);
                let operand = if ends_line {
                    d.indent(operand)
                } else {
                    operand
                };
                let bare_operator =
                    d.concat([Doc::LINE_SUFFIX_BOUNDARY, d.text(member.op.op().spelling())]);
                (Doc::NIL, d.concat([bare_operator, operand, member.after]))
            };
            rest.push(if braced(previous) && hoisted.forced() {
                // The comments keep the operator off the `}` line: both at
                // the level the operator would have had on it.
                d.concat([hoisted, d.snuggle(link)])
            } else if hoisted.forced() {
                d.indent(d.concat([hoisted, d.snuggle(link)]))
            } else if braced(previous) {
                d.concat([Doc::SPACE, link])
            } else if braced(rhs_id) {
                // An operand in braces of its own stays on the operator's
                // line while `op match (x) {` fits there, the braces
                // breaking inside themselves at the chain's level; else the
                // operator starts a line of its own, indented.
                d.layouts([
                    d.concat([Doc::SPACE, link]).broken(),
                    d.indent(d.concat([Doc::HARD_LINE, link])).into(),
                ])
            } else {
                d.indent(d.concat([hoisted, d.snuggle(link)]))
            });
            operands.push(rhs);
            previous = rhs_id;
        }
        d.group(d.concat([
            multi_line_before_last(&operands),
            f.leading_of(first_id),
            first_doc,
            d.concat(rest),
        ]))
    }
}

/// `expr is pattern`; after a comment that ends the expression's line, the
/// `is` starts the next one level in, and so does the pattern after a
/// comment that ends the `is`'s line.
impl FormatNode for IsExpr {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        let d = f.docs;
        d.concat([
            f.alt(node.expr),
            d.indent(d.concat([Doc::SPACE, f.keyword(node.is_kw)])),
            Doc::SPACE,
            f.alt(f.tree.present(node.pattern)),
        ])
    }
}

/// `-x`, `!x`, `~x`. A run of prefix operators (`---x`, `!-x`) is printed by
/// the outermost as one glued run: nothing stands between a prefix operator
/// and its operand, a blank included — at the start of a line `a\n- b`
/// subtracts and `a\n-b` negates, so `--x` after a braced statement stays
/// glued (BAML has no decrement for it to be mistaken for). The comments
/// written inside the run — after an operator, which the comments module
/// attaches to that operator's expression, or carried out of a peeled
/// parenthesis — all go before the first operator, on the first pass rather
/// than one operator further out on each.
impl FormatNode for UnaryExpr {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let d = f.docs;
        let mut ops = Vec::new();
        let mut leading = Vec::new();
        // Each level's operand trailing comments, outermost level first.
        let mut trailing = Vec::new();
        let mut node = f.tree.get(id);
        let base = loop {
            debug_assert!(
                f.dangling_after(node.op.idx).is_empty(),
                "a comment after a prefix operator leads the expression"
            );
            ops.push(node.op.idx);
            let operand = unary_operand(f, f.tree.present(node.operand));
            leading.extend(operand.leading);
            trailing.push(operand.trailing);
            match operand.expr {
                AnyExpr::Unary(inner) => node = f.tree.get(inner),
                base => break base,
            }
        };
        d.concat(
            [
                docs::leading(d, f.tree, &leading),
                Doc::LINE_SUFFIX_BOUNDARY,
            ]
            .into_iter()
            .chain(ops.iter().map(|&op| d.text(f.tree.token_text(op))))
            .chain([f.bare(base.node_id())])
            .chain(
                trailing
                    .iter()
                    .rev()
                    .map(|comments| docs::trailing(d, f.tree, comments)),
            ),
        )
    }
}

/// `kw operand`.
fn keyword_operand<'a, S: baml_syntax::token::TokenSet>(
    f: &Formatter<'a>,
    kw: baml_syntax::token::Token<S>,
    operand: Option<AnyExpr>,
) -> Doc<'a> {
    let d = f.docs;
    d.concat([
        f.token(kw),
        operand.map_or(Doc::NIL, |operand| d.concat([Doc::SPACE, f.alt(operand)])),
    ])
}

impl FormatNode for AwaitExpr {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        keyword_operand(f, node.kw, Some(f.tree.present(node.operand)))
    }
}

impl FormatNode for ThrowExpr {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        keyword_operand(f, node.kw, Some(f.tree.present(node.payload)))
    }
}

impl FormatNode for ReturnExpr {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        keyword_operand(f, node.kw, node.value)
    }
}

impl FormatNode for BreakExpr {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        f.token(f.tree.get(id).kw)
    }
}

impl FormatNode for ContinueExpr {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        f.token(f.tree.get(id).kw)
    }
}

// ---------------------------------------------------------------------------
// Literals
// ---------------------------------------------------------------------------

/// `[a, b]`: all on one line, or one element per line with a trailing
/// comma; the commas are canonical whether or not the source wrote them.
impl FormatNode for ArrayLiteral {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        f.docs.group(array_parts(f, id))
    }
}

fn array_parts<'a>(f: &Formatter<'a>, id: Id<ArrayLiteral>) -> Doc<'a> {
    let node = f.tree.get(id);
    let items: Vec<ListItem<'a>> = f
        .tree
        .list(node.elements)
        .iter()
        .enumerate()
        .map(|(i, element)| {
            f.list_item(
                element.value.node_id(),
                element.comma.map(|comma| comma.idx),
                i == 0,
            )
        })
        .collect();
    f.list_parts(
        AnyNodeId::of(id),
        node.l.idx,
        &items,
        f.tree.present(node.r).idx,
        false,
        f.heuristics.array_elements,
    )
}

/// `{ k: v }` or `map { k: v }`, spaces inside the braces when flat.
impl FormatNode for MapLiteral {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        f.docs
            .concat([map_keyword(f, id), f.docs.group(map_parts(f, id))])
    }
}

/// `map ` when the literal was written with the keyword.
fn map_keyword<'a>(f: &Formatter<'a>, id: Id<MapLiteral>) -> Doc<'a> {
    f.tree
        .get(id)
        .map_kw
        .map_or(Doc::NIL, |kw| f.docs.concat([f.token(kw), Doc::SPACE]))
}

fn map_parts<'a>(f: &Formatter<'a>, id: Id<MapLiteral>) -> Doc<'a> {
    let node = f.tree.get(id);
    let items: Vec<ListItem<'a>> = f
        .tree
        .list(node.entries)
        .iter()
        .enumerate()
        .map(|(i, &entry)| {
            f.list_item(
                AnyNodeId::of(entry),
                f.tree.get(entry).comma.map(|comma| comma.idx),
                i == 0,
            )
        })
        .collect();
    f.list_parts(
        AnyNodeId::of(id),
        node.l.idx,
        &items,
        f.tree.present(node.r).idx,
        true,
        f.heuristics.array_elements,
    )
}

/// `key: value`; its comma is the list's. After a comment that ends the
/// key's line, the `:` starts the next one level in.
impl FormatNode for MapEntry {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        f.docs.concat([
            f.alt(node.key),
            f.continuation(
                node.key.node_id(),
                f.docs.concat([
                    f.token(f.tree.present(node.colon)),
                    Doc::SPACE,
                    f.alt(f.tree.present(node.value)),
                ]),
            ),
        ])
    }
}

/// `Point { x: 1, ...rest }`
impl FormatNode for ObjectLiteral {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        f.docs
            .concat([object_head(f, id), f.docs.group(object_parts(f, id))])
    }
}

/// `Point ` before the braces.
fn object_head<'a>(f: &Formatter<'a>, id: Id<ObjectLiteral>) -> Doc<'a> {
    f.docs.concat([f.alt(f.tree.get(id).ctor), Doc::SPACE])
}

fn object_parts<'a>(f: &Formatter<'a>, id: Id<ObjectLiteral>) -> Doc<'a> {
    let node = f.tree.get(id);
    let items: Vec<ListItem<'a>> = f
        .tree
        .list(node.members)
        .iter()
        .enumerate()
        .map(|(i, &member)| {
            let comma = match member {
                AnyObjectMember::Field(field) => f.tree.get(field).comma,
                AnyObjectMember::Spread(spread) => f.tree.get(spread).comma,
                AnyObjectMember::Bogus(bogus) => f.tree.absent(bogus),
            };
            f.list_item(member.node_id(), comma.map(|comma| comma.idx), i == 0)
        })
        .collect();
    f.list_parts(
        AnyNodeId::of(id),
        node.l.idx,
        &items,
        f.tree.present(node.r).idx,
        true,
        f.heuristics.object_fields,
    )
}

/// `key: value` or the shorthand `key`.
impl FormatNode for ObjectField {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        let key = match node.key {
            AnyObjectKey::Name(name) => f.node(name),
            AnyObjectKey::Bogus(bogus) => f.tree.absent(bogus),
        };
        // After a comment that ends the key's line, the `:` starts the next
        // one level in.
        f.docs.concat([
            key,
            node.value.map_or(Doc::NIL, |value| {
                f.continuation(
                    node.key.node_id(),
                    f.docs.concat([
                        f.token(value.colon),
                        Doc::SPACE,
                        f.alt(f.tree.present(value.value)),
                    ]),
                )
            }),
        ])
    }
}

/// `...value`
impl FormatNode for SpreadElement {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        f.docs
            .concat([f.token(node.dots), f.alt(f.tree.present(node.value))])
    }
}

/// `(a, b)`, or `(a,)` with the comma that tells a one-element tuple from a
/// parenthesised expression; one element per line with a trailing comma
/// when it does not fit.
impl FormatNode for TupleExpr {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        f.tuple(
            node.l,
            f.tree.list(node.elements),
            f.tree.present(node.r),
            |element| element.node_id(),
            |element| f.alt(element),
        )
    }
}

// ---------------------------------------------------------------------------
// Calls
// ---------------------------------------------------------------------------

/// `name = ` of a labelled argument.
fn label<'a>(f: &Formatter<'a>, arg: Id<Arg>) -> Doc<'a> {
    f.tree.get(arg).label.map_or(Doc::NIL, |label| {
        f.docs.concat([
            f.node(label.name),
            Doc::SPACE,
            f.token(label.eq),
            Doc::SPACE,
        ])
    })
}

impl FormatNode for Arg {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        f.docs
            .concat([label(f, id), f.alt(argument(f, f.tree.present(node.value)))])
    }
}

/// An argument in both of its layouts: as one item of the list, and hugged
/// — broken against the parentheses as the last argument, `f(a, (x) -> {`
/// — when it may be: a lambda or `spawn` with its body broken, a `match`,
/// an `if` or a block with its braces broken, a literal with one element
/// per line, or, as the only argument, a call (rustfmt's overflow of a
/// nested call: `f(g(` on one line, `g`'s arguments broken).
/// A multi-line template needs no hugging: it is an atom measured by its
/// first line, so `f(`…`)` stays as it is. Both layouts are built from one
/// printing of the argument's parts, so nested calls cost no more than
/// their size.
struct ArgForms<'a> {
    flat: Doc<'a>,
    hugged: Option<Doc<'a>>,
    /// Whether the hug is rustfmt's overflow of a sole nested call, taken
    /// only once the call does not fit on a line of its own: the shallowest
    /// call that fits breaks, as rustfmt breaks it.
    nested_call: bool,
}

fn arg_forms<'a>(f: &Formatter<'a>, arg: Id<Arg>, sole: bool) -> ArgForms<'a> {
    let d = f.docs;
    let node = f.tree.get(arg);
    let value = argument(f, f.tree.present(node.value));
    let (flat, hugged) = match value {
        AnyExpr::Lambda(id) => {
            let forms = lambda(f, id);
            (
                f.with_comments(id, forms.flat),
                Some(f.with_comments(id, forms.hugged)),
            )
        }
        AnyExpr::Spawn(id) => {
            let forms = spawn(f, id);
            (
                f.with_comments(id, forms.flat),
                Some(f.with_comments(id, forms.hugged)),
            )
        }
        AnyExpr::Array(id) => {
            let parts = array_parts(f, id);
            (
                f.with_comments(id, d.group(parts)),
                Some(f.with_comments(id, d.expanded_group(parts))),
            )
        }
        AnyExpr::Map(id) => {
            let kw = map_keyword(f, id);
            let parts = map_parts(f, id);
            (
                f.with_comments(id, d.concat([kw, d.group(parts)])),
                Some(f.with_comments(id, d.concat([kw, d.expanded_group(parts)]))),
            )
        }
        AnyExpr::Object(id) => {
            let head = object_head(f, id);
            let parts = object_parts(f, id);
            (
                f.with_comments(id, d.concat([head, d.group(parts)])),
                Some(f.with_comments(id, d.concat([head, d.expanded_group(parts)]))),
            )
        }
        AnyExpr::Call(id) if sole && matches!(f.tree.get(id).callee, AnyExpr::Path(_)) => {
            let chain = Chain::of(f, value).doc(f, Doc::NIL);
            (
                f.with_comments(id, chain),
                Some(f.with_comments(id, d.expanded_group(chain))),
            )
        }
        // A `match`, an `if` or a block hugs as a lambda does: its braces
        // against the parenthesis, broken inside.
        AnyExpr::Match(id) => (f.node(id), Some(f.node(id))),
        AnyExpr::If(id) => (
            f.with_comments(id, if_chain(f, IfHead::If(id), true)),
            Some(f.with_comments(id, if_chain(f, IfHead::If(id), false))),
        ),
        AnyExpr::IfLet(id) => (
            f.with_comments(id, if_chain(f, IfHead::IfLet(id), true)),
            Some(f.with_comments(id, if_chain(f, IfHead::IfLet(id), false))),
        ),
        AnyExpr::Block(id) => {
            let (flat, broken) = f.block_forms(id);
            (flat, Some(broken))
        }
        other => (f.alt(other), None),
    };
    let nested_call = hugged.is_some() && matches!(value, AnyExpr::Call(_));
    let label = label(f, arg);
    ArgForms {
        flat: f.with_comments(arg, d.concat([label, flat])),
        hugged: hugged.map(|hugged| f.with_comments(arg, d.concat([label, hugged]))),
        nested_call,
    }
}

/// The arguments of one call in every layout a chain needs, built once and
/// shared by the layouts.
struct CallArgs<'a> {
    /// The list's own choice: all on one line; else the last argument
    /// hugged when it may be; else one argument per line.
    choice: Doc<'a>,
    /// One argument per line, for a chain layout that breaks only here.
    broken: Doc<'a>,
    /// The last argument hugged, when it may be.
    hugged: Option<Doc<'a>>,
    /// Whether that hug is a sole nested call's (see [`ArgForms`]): the
    /// list, and a chain ending in it, break one argument per line first and
    /// hug only when the call does not fit on a line of its own — or is sure
    /// not to, a lambda's block among its arguments, when the hug is where
    /// the break belongs.
    nested_call: bool,
}

impl<'a> CallArgs<'a> {
    fn of(f: &Formatter<'a>, id: Id<ArgList>) -> Self {
        let d = f.docs;
        let node = f.tree.get(id);
        let args = f.tree.list(node.args);
        let r = f.tree.present(node.r);
        let forms: Vec<ArgForms<'a>> = args
            .iter()
            .map(|arg| arg_forms(f, arg.item, args.len() == 1))
            .collect();
        let items: Vec<ListItem<'a>> = args
            .iter()
            .zip(&forms)
            .enumerate()
            .map(|(i, (arg, form))| ListItem {
                doc: form.flat,
                comma: arg.sep.map(|comma| comma.idx),
                blank_before: i > 0
                    && f.comments
                        .gap_before_node(f.tree, AnyNodeId::of(arg.item))
                        .has_blank_line(),
            })
            .collect();
        let parts = f.delimited_items(node.l.idx, &items, r, Some(f.heuristics.call_args));
        let hugged = hug(f, node.l.idx, args, &items, &forms, r);
        let nested_call = forms.last().is_some_and(|form| form.nested_call);
        let choice = match hugged {
            Some(hug) if nested_call && !parts.will_break() => d.best_fitting([
                parts.into(),
                d.expanded_group(parts).all_lines_flat(),
                hug.into(),
                d.group(parts).into(),
            ]),
            Some(hug) => d.best_fitting([parts, hug, d.group(parts)]),
            None => d.group(parts),
        };
        Self {
            choice,
            broken: d.expanded_group(parts),
            hugged,
            nested_call,
        }
    }
}

/// The hug layout of `(args)`: the arguments before the last on one line,
/// the last broken against the closing parenthesis, that line within the
/// width of a call's arguments as the flat layout is. None when the last
/// may not be hugged, or when something before it would not keep to that
/// one line: an argument sure to break, a comment that ends a line or leads
/// the last argument on a line of its own, a blank line the author left —
/// or when comments follow the last argument, or its comma, which would sit
/// between it and the closing parenthesis.
fn hug<'a>(
    f: &Formatter<'a>,
    l: TokenIdx,
    args: &[SepItem<Id<Arg>, punct::Comma>],
    items: &[ListItem<'a>],
    forms: &[ArgForms<'a>],
    r: Token![")"],
) -> Option<Doc<'a>> {
    let d = f.docs;
    let (last, init) = args.split_last()?;
    let hugged = forms.last()?.hugged?;
    let open = d.concat([f.comments_inside(l), f.space_after_opener(l)]);
    let before: Vec<Doc<'a>> = init
        .iter()
        .zip(items)
        .map(|(arg, item)| {
            d.concat([
                item.doc,
                f.separator_at(arg.sep.map(|comma| comma.idx), ","),
                Doc::SPACE,
            ])
        })
        .collect();
    let leading_last = f.leading_of(AnyNodeId::of(last.item));
    if open.will_break()
        || items.iter().any(|item| item.blank_before)
        || before.iter().any(|doc| doc.spans_lines())
        || leading_last.will_break()
        || f.trailing_of(AnyNodeId::of(last.item)).will_break()
        || last
            .sep
            .is_some_and(|comma| f.has_comments_after(comma.idx))
    {
        return None;
    }
    let inside = d.concat([
        open,
        d.concat(before),
        hugged,
        last.sep
            .map_or(Doc::NIL, |comma| f.comments_after(comma.idx)),
    ]);
    Some(d.concat([
        d.text(f.tree.token_text(l)),
        d.limited(inside, f.heuristics.call_args),
        f.token(r),
    ]))
}

/// `(args)`: all on one line; else the last argument hugged when it may be;
/// else one argument per line with a trailing comma.
impl FormatNode for ArgList {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        CallArgs::of(f, id).choice
    }
}

/// `[index]`, the index on a line of its own when it does not fit.
fn index_parts<'a>(f: &Formatter<'a>, l: TokenIdx, index: AnyExpr, r: TokenIdx) -> Doc<'a> {
    let d = f.docs;
    d.concat([
        d.text(f.tree.token_text(l)),
        d.indent(d.concat([f.comments_inside(l), f.line_after_opener(l), f.alt(index)])),
        Doc::SOFT_LINE,
        d.text(f.tree.token_text(r)),
        f.comments_after(r),
    ])
}

// ---------------------------------------------------------------------------
// Member chains
// ---------------------------------------------------------------------------

/// One step of a postfix chain after its receiver.
enum Link<'a> {
    /// `.name`, `?.name`, `<T>`, `.upcast<T>()`, `@spec`, a tagged template: glued
    /// to what it follows. A `field` is a named access, where the expanded
    /// layout may start a line; a `type_name` is one spelled as a type is
    /// (`Duration` in `baml.time.Duration.from_seconds(…)`), so that a call
    /// right after it is a static call, part of the receiver (see
    /// [`is_type_name`]).
    Plain {
        doc: Doc<'a>,
        field: bool,
        type_name: bool,
    },
    /// `(args)` or `?.(args)`; `optional` when the latter.
    Call {
        prefix: Doc<'a>,
        optional: bool,
        args: CallArgs<'a>,
    },
    /// `[i]` or `?.[i]`; `optional` when the latter.
    Index {
        prefix: Doc<'a>,
        optional: bool,
        parts: Doc<'a>,
    },
}

impl<'a> Link<'a> {
    fn is_plain(&self) -> bool {
        matches!(self, Self::Plain { .. })
    }

    fn is_field(&self) -> bool {
        matches!(self, Self::Plain { field: true, .. })
    }

    /// Whether the link may start a line: a named access, or a call or
    /// index after `?.` — a bare `(` or `[` after a line break would not
    /// read as part of the chain.
    fn can_start_line(&self) -> bool {
        match *self {
            Self::Plain { field, .. } => field,
            Self::Call { optional, .. } | Self::Index { optional, .. } => optional,
        }
    }

    fn is_type_name(&self) -> bool {
        matches!(
            self,
            Self::Plain {
                type_name: true,
                ..
            }
        )
    }

    /// The link as it prints on a line.
    fn doc(&self, d: Docs<'a>) -> Doc<'a> {
        match *self {
            Self::Plain { doc, .. } => doc,
            Self::Call {
                prefix, ref args, ..
            } => d.concat([prefix, args.choice]),
            Self::Index { prefix, parts, .. } => d.concat([prefix, d.group(parts)]),
        }
    }
}

/// A link that may start a line — `.name`, `?.`, `.0`, `.upcast<T>()` — as `dot`
/// (the token, with the comments dangling after it) and `rest`: one level
/// in when it starts a line after a comment that ended the line of what it
/// follows (`x // c` then `    .y`), and so is what follows a line comment
/// on the dot's line (`x. // c` then `    y`) — one level, not two: the
/// dot's comments are printed as a keyword's are, the indentation is the
/// link's own. In a chain's expanded layout the links are indented already;
/// a line break inside one then goes one level past the link's line.
fn dotted<'a, S: baml_syntax::token::TokenSet>(
    f: &Formatter<'a>,
    dot: Token<S>,
    rest: Doc<'a>,
) -> Doc<'a> {
    let d = f.docs;
    d.indent(d.concat([f.keyword(dot), rest]))
}

/// The receiver a chain starts from and the links after it.
struct Chain<'a> {
    first: Doc<'a>,
    /// The receiver's trailing comments, placed by the layout: inside the
    /// chain's indentation when the chain breaks, so that a comment between
    /// the receiver and its first `.call()` sits with the call.
    first_trailing: Doc<'a>,
    /// Whether the receiver ends in brackets of its own that break inside
    /// themselves (see [`block_like`]).
    first_block_like: bool,
    /// Whether the receiver is a name spelled as a type is (see
    /// [`is_type_name`]).
    first_type_name: bool,
    links: Vec<Link<'a>>,
}

/// Whether a name is spelled as a type is — `Agent`, `Duration` — so that
/// a call right after it (`Agent.new(…)`, `Duration.from_seconds(…)`) is a
/// static call: the chain's receiver, as `Agent::new(…)` is the root of a
/// chain for rustfmt, not a method link that may start a line of its own.
/// Classes, enums and interfaces are spelled in `UpperCamelCase` and
/// nothing else is, so the spelling tells.
fn is_type_name(name: &str) -> bool {
    name.chars().next().is_some_and(char::is_uppercase)
}

/// The lines of a chain's expanded layout: the receiver's, which keeps the
/// calls made directly on it, the plain accesses up to the first call's
/// name, and that first call when it is a static call on a type
/// (`Agent.new(…)`, see [`is_type_name`]); then one per call after it, the
/// plain accesses between calls glued to the call they precede and those
/// after the last call a line of their own. A line can only start with a
/// named access (`.name`): a call, an index, a generic application, `@spec`
/// or a tagged template after a line break would not parse as part of the
/// chain, so a run with no named access before its calls joins the line
/// before it.
struct Lines<'a> {
    head: Doc<'a>,
    /// Whether the receiver's line ends in brackets of its own (see
    /// [`block_like`]).
    head_block_like: bool,
    /// The receiver's trailing comments when they end its line: they go
    /// inside the indentation, with the first call.
    inner_trailing: Doc<'a>,
    /// One per line after the receiver's.
    calls: Vec<Doc<'a>>,
    /// How many of those lines hold a call or an index — rustfmt's count of
    /// a chain's children, for its chain width: a static call on a type
    /// (`Type.new(…)`) is the receiver's, and a trailing `.field` line is no
    /// call.
    children: usize,
}

/// Whether an expression ends in brackets of its own that break inside
/// themselves — a literal with elements, or anything [`braced`] — leaving
/// only closing brackets on its last line when it does: rustfmt's block-like
/// parent of a chain.
fn block_like(id: AnyNodeId) -> bool {
    braced(id)
        || matches!(
            id.typed(),
            AnyId::ObjectLiteral(_) | AnyId::ArrayLiteral(_) | AnyId::MapLiteral(_)
        )
}

/// One step down a postfix spine (see [`Chain::of`]).
enum Next {
    /// A postfix node's base: walked further.
    Walk(AnyExpr),
    /// The receiver, taken as it is.
    Receiver(AnyExpr),
}
use Next::{Receiver, Walk};

/// Whether a tuple slot's base is itself a slot, through transparent
/// parentheses: `.1.0` would read as one float slot.
fn tuple_slot_on_slot(f: &Formatter<'_>, access: Id<TupleFieldExpr>) -> bool {
    matches!(
        peel(f, f.tree.get(access).base, |_| true),
        AnyExpr::TupleField(_)
    )
}

impl<'a> Chain<'a> {
    /// The chain `expr` is the end of, built down to its receiver. The
    /// expressions along the spine are not printed as nodes of their own,
    /// so their trailing comments are added here, after the link that
    /// closes each; `expr` itself is printed as a node by its caller. The
    /// spine is walked with a loop — a generated chain of any length costs
    /// no stack — outermost in, and the links are then pushed innermost out.
    fn of(f: &Formatter<'a>, expr: AnyExpr) -> Self {
        let tree = f.tree;
        let mut chain = Self {
            first: Doc::NIL,
            first_trailing: Doc::NIL,
            first_block_like: false,
            first_type_name: false,
            links: Vec::new(),
        };
        // The postfix nodes from `expr` down to the receiver, outermost
        // first, each without the redundant parentheses around it.
        let mut spine: Vec<AnyExpr> = Vec::new();
        let mut at = postfix_receiver(f, expr);
        let receiver = loop {
            let next = match at {
                // A path is its own receiver: a name and its segments.
                AnyExpr::Path(_) => break None,
                // What a call or a generic application applies to is walked
                // like any receiver, unless it is in parentheses, which stay
                // whatever they hold — `(v.f)(3)` calls the value of field
                // `f` where `v.f(3)` calls the method — and so is the
                // receiver, parentheses and all.
                AnyExpr::Call(_) | AnyExpr::OptionalCall(_) | AnyExpr::GenericApply(_) => {
                    match postfix_base(tree, at) {
                        Some(callee @ AnyExpr::Paren(_)) => Receiver(callee),
                        Some(callee) => Walk(callee),
                        None => unreachable!("a call has a callee"),
                    }
                }
                // `(env).NAME` reads a field of a local named `env`; bare, it
                // would read the environment, so the parentheses stay.
                AnyExpr::FieldAccess(access) if is_bare_env(f, tree.get(access).base) => {
                    Receiver(tree.get(access).base)
                }
                // A slot after a slot: `.1.0` would read as one float slot,
                // so parentheses around the base stay (`(t.1).0`).
                AnyExpr::TupleField(access)
                    if matches!(tree.get(access).base, AnyExpr::Paren(_))
                        && tuple_slot_on_slot(f, access) =>
                {
                    Receiver(tree.get(access).base)
                }
                other => match postfix_base(tree, other) {
                    Some(base) => Walk(base),
                    None => break Some(other),
                },
            };
            spine.push(at);
            match next {
                Walk(base) => at = postfix_receiver(f, base),
                Receiver(receiver) => break Some(receiver),
            }
        };
        match receiver {
            Some(receiver) => chain.receiver(f, receiver.node_id()),
            None => {
                let AnyExpr::Path(path) = at else {
                    unreachable!("the loop stops on a path only with one")
                };
                let node = tree.get(path);
                chain.receiver(f, AnyNodeId::of(node.head));
                chain.first_type_name = is_type_name(f.node_text(AnyNodeId::of(node.head)));
                for segment in tree.list(node.segments) {
                    chain
                        .links
                        .push(Self::named(f, segment.dot, tree.present(segment.name)));
                }
                // The path's own trailing comments, unless it is `expr`
                // itself, whose caller prints them.
                if !spine.is_empty() {
                    chain.trailing(f, AnyNodeId::of(path));
                }
            }
        }
        let outermost = spine.first().copied();
        for &step in spine.iter().rev() {
            chain.link(f, step);
            if Some(step) != outermost {
                chain.trailing(f, step.node_id());
            }
        }
        chain
    }

    /// The trailing comments of a spine node, after the link that closes
    /// it.
    fn trailing(&mut self, f: &Formatter<'a>, id: AnyNodeId) {
        let d = f.docs;
        let trailing = f.trailing_of(id);
        match self.links.last_mut() {
            Some(Link::Plain { doc, .. }) => *doc = d.concat([*doc, trailing]),
            // Comments after a call are a link of their own, glued to it;
            // none makes no link, so that the call stays the last on its
            // line (see [`Lines::head_block_like`]).
            Some(Link::Call { .. } | Link::Index { .. }) => {
                if !f.comments.trailing(id).is_empty() {
                    self.links.push(Link::Plain {
                        doc: trailing,
                        field: false,
                        type_name: false,
                    });
                }
            }
            None => self.first_trailing = d.concat([self.first_trailing, trailing]),
        }
    }

    /// The link `step`, a postfix node of the spine, adds after its base.
    fn link(&mut self, f: &Formatter<'a>, step: AnyExpr) {
        let tree = f.tree;
        let d = f.docs;
        match step {
            AnyExpr::GenericApply(apply) => {
                let node = tree.get(apply);
                self.links.push(Link::Plain {
                    doc: f.node(node.args),
                    field: false,
                    type_name: false,
                });
            }
            AnyExpr::FieldAccess(access) => {
                let node = tree.get(access);
                self.links
                    .push(Self::named(f, node.dot, tree.present(node.name)));
            }
            AnyExpr::OptionalFieldAccess(access) => {
                let node = tree.get(access);
                self.links
                    .push(Self::named(f, node.qdot, tree.present(node.name)));
            }
            AnyExpr::TupleField(access) => {
                let node = tree.get(access);
                // A bare base that is a slot keeps the space that separated
                // them (`t.1 .0`): `.1.0` would read as one float slot.
                let space = match (node.base, tuple_slot_on_slot(f, access)) {
                    (AnyExpr::Paren(_), _) | (_, false) => Doc::NIL,
                    (_, true) => Doc::SPACE,
                };
                self.links.push(Link::Plain {
                    doc: d.concat([space, dotted(f, node.dot, f.token(node.slot))]),
                    field: true,
                    type_name: false,
                });
            }
            AnyExpr::Upcast(upcast) => {
                let node = tree.get(upcast);
                // The parentheses are the expression's own and hold nothing:
                // they stay glued to the type arguments.
                let call = node.call.map_or(Doc::NIL, |call| f.node(call));
                self.links.push(Link::Plain {
                    doc: dotted(
                        f,
                        node.dot,
                        d.concat([
                            f.token(node.upcast_kw),
                            f.node(tree.present(node.args)),
                            call,
                        ]),
                    ),
                    field: true,
                    type_name: false,
                });
            }
            AnyExpr::Spec(spec) => {
                let node = tree.get(spec);
                self.links.push(Link::Plain {
                    // `@` cannot start a line: it tucks in before a pending comment.
                    doc: d.concat([
                        d.text(tree.token_text(node.at.idx)),
                        f.comments_after(node.at.idx),
                        f.token(tree.present(node.selector)),
                    ]),
                    field: false,
                    type_name: false,
                });
            }
            AnyExpr::TaggedTemplate(tagged) => {
                let node = tree.get(tagged);
                self.links.push(Link::Plain {
                    doc: f.node(node.template),
                    field: false,
                    type_name: false,
                });
            }
            AnyExpr::Call(call) => {
                let node = tree.get(call);
                self.links.push(Link::Call {
                    prefix: Doc::NIL,
                    optional: false,
                    args: CallArgs::of(f, node.args),
                });
            }
            AnyExpr::OptionalCall(call) => {
                let node = tree.get(call);
                // A comment between `?.` and `(` leads the argument list,
                // which prints its parts but not its comments: `g?. /* c */ (1)`.
                let args_id = AnyNodeId::of(node.args);
                let leading = if f.comments.leading(args_id).is_empty() {
                    Doc::NIL
                } else {
                    d.concat([Doc::SPACE, f.leading_of(args_id)])
                };
                self.links.push(Link::Call {
                    prefix: dotted(f, node.qdot, leading),
                    optional: true,
                    args: CallArgs::of(f, node.args),
                });
            }
            AnyExpr::Index(index) => {
                let node = tree.get(index);
                self.links.push(Link::Index {
                    prefix: Doc::NIL,
                    optional: false,
                    parts: index_parts(
                        f,
                        node.l.idx,
                        tree.present(node.index),
                        tree.present(node.r).idx,
                    ),
                });
            }
            AnyExpr::OptionalIndex(index) => {
                let node = tree.get(index);
                self.links.push(Link::Index {
                    prefix: dotted(f, node.qdot, Doc::NIL),
                    optional: true,
                    parts: index_parts(
                        f,
                        node.l.idx,
                        tree.present(node.index),
                        tree.present(node.r).idx,
                    ),
                });
            }
            AnyExpr::Path(_) => unreachable!("a path is the chain's receiver"),
            other => unreachable!("{other:?} is not a postfix node"),
        }
    }

    /// The receiver: its leading comments and itself; its trailing comments
    /// are kept for the layout.
    fn receiver(&mut self, f: &Formatter<'a>, id: AnyNodeId) {
        self.first = f.docs.concat([f.leading_of(id), f.bare(id)]);
        self.first_trailing = f.trailing_of(id);
        self.first_block_like = block_like(id);
    }

    /// `.name`
    fn named<S: baml_syntax::token::TokenSet>(
        f: &Formatter<'a>,
        dot_token: Token<S>,
        name: Id<baml_syntax::nodes::Name>,
    ) -> Link<'a> {
        Link::Plain {
            doc: dotted(f, dot_token, f.node(name)),
            field: true,
            type_name: is_type_name(f.tree.token_text(f.tree.get(name).token.idx)),
        }
    }

    /// The chain laid out, `tail` after it on its last line: on one line;
    /// the last call's last argument hugged; every call on its own line when
    /// each line then fits whole; only the last call's (or index's)
    /// brackets broken, the plain accesses after it following; every call
    /// on its own line. A chain of two or more calls — one whose expanded
    /// layout has two or more call lines after the receiver's: rustfmt's
    /// count of a chain's children, a dotted path before the first call
    /// being a name rather than a chain and a trailing `.field` no call —
    /// keeps to rustfmt's chain width in the layouts
    /// that keep it on one line, up to the last call's broken or hugged
    /// arguments. A forced break before the last call (a multi-line lambda,
    /// a comment) leaves only the last.
    fn doc(&self, f: &Formatter<'a>, tail: Doc<'a>) -> Doc<'a> {
        let d = f.docs;
        let lines = self.lines(d);
        let links = |links: &[Link<'a>]| d.concat(links.iter().map(|link| link.doc(d)));
        let Some(last_at) = self.links.iter().rposition(|link| !link.is_plain()) else {
            return d.concat([
                self.first,
                d.indent(self.first_trailing),
                links(&self.links),
                tail,
            ]);
        };
        let (prefix, rest) = self.links.split_at(last_at);
        let (last, suffix) = rest
            .split_first()
            .unwrap_or_else(|| unreachable!("`last_at` indexes a link"));
        let before_last = d.concat([self.first, self.first_trailing, links(prefix)]);
        if before_last.will_break() {
            return Self::expanded(d, &lines, tail);
        }
        let within = |doc: Doc<'a>| {
            if lines.children >= 2 {
                d.limited(doc, f.heuristics.chain)
            } else {
                doc
            }
        };
        let after_last = links(suffix);
        let on_a_line =
            |last: Doc<'a>| d.concat([within(d.concat([before_last, last, after_last])), tail]);
        let flat = on_a_line(last.doc(d));
        let expanded = Self::expanded(d, &lines, tail);
        let nested_call = matches!(*last, Link::Call { ref args, .. } if args.nested_call);
        let (tail_broken, hug) = match *last {
            Link::Call {
                prefix: qdot,
                ref args,
                ..
            } => (
                on_a_line(d.concat([qdot, args.broken])),
                args.hugged.map(|hug| on_a_line(d.concat([qdot, hug]))),
            ),
            Link::Index {
                prefix: qdot,
                parts,
                ..
            } => (on_a_line(d.concat([qdot, d.expanded_group(parts)])), None),
            Link::Plain { .. } => unreachable!("`last_at` is not a plain link"),
        };
        // The flat variant may hold a forced break — the last call's
        // arguments break for a multi-line lambda, the tail's `catch` arms
        // break — and still print the chain itself on one line: those parts
        // lay themselves out in any mode (a forced group prints broken, a
        // hard line breaks). What came before the last call was checked
        // above. The groups around the chain must know, though, that it is
        // sure to break.
        let choice = match hug {
            // A sole nested call hugs only once it does not fit on a line of
            // its own: the last call's arguments break first.
            Some(hug) if nested_call => d.layouts([
                flat.into(),
                tail_broken.all_lines_flat(),
                hug.into(),
                expanded.all_lines(),
                tail_broken.into(),
                expanded.into(),
            ]),
            Some(hug) => d.layouts([
                flat.into(),
                hug.into(),
                expanded.all_lines(),
                tail_broken.into(),
                expanded.into(),
            ]),
            None => d.layouts([
                flat.into(),
                expanded.all_lines(),
                tail_broken.into(),
                expanded.into(),
            ]),
        };
        if flat.forced() {
            d.concat([Doc::EXPAND_PARENT, choice])
        } else {
            choice
        }
    }

    /// The lines of the expanded layout (see [`Lines`]).
    fn lines(&self, d: Docs<'a>) -> Lines<'a> {
        let mut rest: &[Link<'a>] = &self.links;
        // The receiver's trailing comments stay in order: on its line after
        // it when they fit there; when they end the line, nothing is glued
        // to the receiver and the comments go inside the indentation, with
        // the first call, so that what follows them starts a line one level
        // in.
        let trailing_ends_line = self.first_trailing.will_break();
        let mut head = vec![self.first];
        if !trailing_ends_line {
            head.push(self.first_trailing);
        }
        // The last link on the receiver's line, which decides whether that
        // line ends in brackets of its own.
        let mut last_on_head: Option<&Link<'a>> = None;
        while let Some((link, after)) = rest.split_first() {
            // After a comment that ends the receiver's line, a link that
            // can start a line does.
            if link.is_plain() || (trailing_ends_line && link.can_start_line()) {
                break;
            }
            head.push(link.doc(d));
            last_on_head = Some(link);
            rest = after;
        }
        let plain_run = rest.iter().take_while(|link| link.is_plain()).count();
        // A link that ends its line (a call's trailing line comment) ends
        // the receiver's line: nothing after it is glued to it.
        let ends_line = rest[..plain_run]
            .iter()
            .position(|link| link.doc(d).will_break())
            .map(|at| at + 1);
        let glued = if trailing_ends_line {
            0
        } else if let Some(ends_line) = ends_line {
            ends_line
        } else if plain_run == rest.len() {
            plain_run
        } else {
            match rest[..plain_run].iter().rposition(Link::is_field) {
                None => plain_run,
                Some(name_at) => {
                    // The first call's name starts a line — unless what it
                    // is called on is a type, when the call is a static one
                    // and stays with the receiver (see [`is_type_name`]).
                    let on_a_type = match name_at.checked_sub(1) {
                        Some(before) => rest[before].is_type_name(),
                        None => self.first_type_name,
                    };
                    if on_a_type { plain_run } else { name_at }
                }
            }
        };
        let inner_trailing = if trailing_ends_line {
            self.first_trailing
        } else {
            Doc::NIL
        };
        head.extend(rest[..glued].iter().map(|link| link.doc(d)));
        last_on_head = rest[..glued].last().or(last_on_head);
        rest = &rest[glued..];
        let mut calls: Vec<Vec<Doc<'a>>> = Vec::new();
        let mut children = 0;
        while !rest.is_empty() {
            let plain = rest.iter().take_while(|link| link.is_plain()).count();
            let callish = rest[plain..]
                .iter()
                .take_while(|link| !link.is_plain())
                .count();
            let (run, after) = rest.split_at(plain + callish);
            rest = after;
            // The run starts a line at its first link that can (a named
            // access, a `?.` call or index); what comes before that stays
            // on the line before.
            let starts = run.iter().position(Link::can_start_line);
            let (joined, call) = run.split_at(starts.unwrap_or(run.len()));
            let joined_docs = joined.iter().map(|link| link.doc(d));
            match calls.last_mut() {
                Some(previous) => previous.extend(joined_docs),
                None => {
                    head.extend(joined_docs);
                    last_on_head = joined.last().or(last_on_head);
                }
            }
            if !call.is_empty() {
                calls.push(call.iter().map(|link| link.doc(d)).collect());
                children += usize::from(callish > 0);
            }
        }
        Lines {
            head: d.concat(head),
            head_block_like: last_on_head.map_or(self.first_block_like, |link| !link.is_plain()),
            inner_trailing,
            calls: calls.into_iter().map(|call| d.concat(call)).collect(),
            children,
        }
    }

    /// Every call on a line of its own, `tail` on the last of them. The
    /// calls are indented under the receiver — unless the receiver's line
    /// ends in brackets of its own that broke, leaving only closing brackets
    /// on its last line: the calls then sit at the receiver's indent, as
    /// rustfmt puts them after a block-like parent, where one level in would
    /// read as part of what the brackets closed. Whether the line broke is
    /// the printer's to know: it is a named group, and the calls go one
    /// level in only while it is flat.
    fn expanded(d: Docs<'a>, lines: &Lines<'a>, tail: Doc<'a>) -> Doc<'a> {
        if lines.calls.is_empty() {
            return d.concat([lines.head, lines.inner_trailing, tail]);
        }
        let calls = d.concat(
            std::iter::once(lines.inner_trailing)
                .chain(lines.calls.iter().flat_map(|&call| [Doc::HARD_LINE, call]))
                .chain([tail]),
        );
        if !lines.head_block_like {
            return d.concat([lines.head, d.indent(calls)]);
        }
        let (head, head_group) = d.group_with_id(lines.head);
        d.concat([head, d.if_group_breaks(head_group, calls, d.indent(calls))])
    }
}

/// `expr` with `tail` after it on its last line: for a member chain, inside
/// the chain's indentation when the chain breaks, so that a `catch` after a
/// broken chain hangs off its last call rather than sitting at the indent
/// of the chain's first line; for anything else, simply after it. After a
/// comment that ends the expression's line, the tail goes one level in.
pub(super) fn with_tail<'a>(f: &Formatter<'a>, expr: AnyExpr, tail: Doc<'a>) -> Doc<'a> {
    let d = f.docs;
    let id = expr.node_id();
    // After a comment that ends the expression's line, the tail starts the
    // next one level in.
    let tail = f.continuation(id, tail);
    if is_chain(expr) {
        d.concat([
            f.leading_of(id),
            Chain::of(f, expr).doc(f, d.concat([f.trailing_of(id), tail])),
        ])
    } else {
        d.concat([f.alt(expr), tail])
    }
}

/// The expressions laid out as a member chain: each gets its rule from the
/// one list, and [`is_chain`] answers for the same list.
macro_rules! chain_rules {
    ($($node:ident => $variant:ident),* $(,)?) => {
        $(impl FormatNode for $node {
            fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
                Chain::of(f, AnyExpr::$variant(id)).doc(f, Doc::NIL)
            }
        })*

        /// Whether `expr` is laid out as a member chain.
        fn is_chain(expr: AnyExpr) -> bool {
            matches!(expr, $(AnyExpr::$variant(_))|*)
        }
    };
}

chain_rules! {
    TupleFieldExpr => TupleField,
    PathExpr => Path,
    GenericApplyExpr => GenericApply,
    FieldAccessExpr => FieldAccess,
    OptionalFieldAccessExpr => OptionalFieldAccess,
    UpcastExpr => Upcast,
    SpecExpr => Spec,
    TaggedTemplateExpr => TaggedTemplate,
    CallExpr => Call,
    OptionalCallExpr => OptionalCall,
    IndexExpr => Index,
    OptionalIndexExpr => OptionalIndex,
}

#[cfg(test)]
mod tests {
    use baml_syntax::ops::BinaryOp;

    use super::chain_row;

    /// The chaining rows are the precedence table's: two operators share a
    /// row exactly when they bind alike, and none but the comparisons, which
    /// do not chain, is left out.
    #[test]
    fn chain_rows_follow_the_binding_powers() {
        for &a in BinaryOp::ALL {
            for &b in BinaryOp::ALL {
                if let (Some(row_a), Some(row_b)) = (chain_row(a), chain_row(b)) {
                    assert_eq!(
                        row_a == row_b,
                        a.binding_power() == b.binding_power(),
                        "{a:?} and {b:?}"
                    );
                }
            }
            let comparison = matches!(
                a,
                BinaryOp::Eq
                    | BinaryOp::Ne
                    | BinaryOp::Lt
                    | BinaryOp::Gt
                    | BinaryOp::Le
                    | BinaryOp::Ge
                    | BinaryOp::Instanceof
            );
            assert_eq!(chain_row(a).is_none(), comparison, "{a:?}");
        }
    }
}
