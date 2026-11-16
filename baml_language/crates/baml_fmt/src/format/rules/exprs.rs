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
//! line; the last call's last argument hugged (a lambda, `spawn`, object,
//! array or map, or a sole nested call); every call on its own line under
//! the receiver, plain accesses glued to the call before them, when each of
//! those lines then fits whole; only the last call's arguments broken; else
//! every call on its own line with the arguments breaking as they must.
//! Every layout shares one printing of the arguments, so nesting costs no
//! more than its size.

use baml_syntax::{
    nodes::{
        AnyExpr, AnyObjectKey, AnyObjectMember, Arg, ArgList, ArrayLiteral, AwaitExpr, BinaryExpr,
        BreakExpr, CallExpr, ContinueExpr, EnvAccessExpr, FieldAccessExpr, GenericApplyExpr,
        IndexExpr, IsExpr, LiteralExpr, MapEntry, MapLiteral, ObjectField, ObjectLiteral,
        OptionalCallExpr, OptionalFieldAccessExpr, OptionalIndexExpr, ParenExpr, PathExpr,
        ReturnExpr, SpecExpr, SpreadElement, StringLit, TaggedTemplateExpr, ThrowExpr, TupleExpr,
        TupleFieldExpr, UnaryExpr, UpcastExpr,
    },
    ops::{BinaryOp, BinaryOpTok},
    token::{Token, TokenIdx, TokenKind, punct},
    tree::{AnyNodeId, Id, NodeUnion as _, SepItem},
};

use super::control::{lambda, spawn};
use crate::{
    comments::docs,
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
/// without parentheses.
fn binds_as_postfix_operand(f: &Formatter<'_>, expr: AnyExpr) -> bool {
    match expr {
        AnyExpr::Call(_) | AnyExpr::Index(_) | AnyExpr::FieldAccess(_) | AnyExpr::TupleField(_) => {
            !has_optional_link(f, expr)
        }
        AnyExpr::Path(_)
        | AnyExpr::EnvAccess(_)
        | AnyExpr::Array(_)
        | AnyExpr::String(_)
        | AnyExpr::Backtick(_) => true,
        _ => false,
    }
}

/// Whether an optional access (`?.`) is on the spine of `expr`: parentheses
/// around it stop the short-circuit at that point, so they stay.
fn has_optional_link(f: &Formatter<'_>, expr: AnyExpr) -> bool {
    let tree = f.tree;
    match expr {
        AnyExpr::OptionalFieldAccess(_) | AnyExpr::OptionalIndex(_) | AnyExpr::OptionalCall(_) => {
            true
        }
        AnyExpr::Call(call) => has_optional_link(f, tree.get(call).callee),
        AnyExpr::Index(index) => has_optional_link(f, tree.get(index).base),
        AnyExpr::FieldAccess(access) => has_optional_link(f, tree.get(access).base),
        AnyExpr::TupleField(access) => {
            let node = tree.get(access);
            tree.tokens().kind(node.dot.idx) == TokenKind::QuestionDot
                || has_optional_link(f, node.base)
        }
        AnyExpr::Paren(paren) => has_optional_link(f, tree.present(tree.get(paren).inner)),
        _ => false,
    }
}

/// Whether `expr` is a member access — a path with a segment, a field or
/// tuple slot access — which, as a callee, needs the parentheses it was
/// written in: `(v.f)(3)` calls the value of field `f`, `v.f(3)` calls the
/// method `f`.
fn is_member(f: &Formatter<'_>, expr: AnyExpr) -> bool {
    match expr {
        AnyExpr::Path(path) => !f.tree.list(f.tree.get(path).segments).is_empty(),
        AnyExpr::FieldAccess(_) | AnyExpr::OptionalFieldAccess(_) | AnyExpr::TupleField(_) => true,
        _ => false,
    }
}

/// Whether `expr` is a member access in parentheses that a call needs
/// (see [`is_member`]): the parentheses then stay, and the call's receiver
/// is the parenthesised expression as a whole.
fn is_grouped_member(f: &Formatter<'_>, expr: AnyExpr) -> bool {
    matches!(expr, AnyExpr::Paren(_)) && is_member(f, peel(f, expr, |_| true))
}

/// A receiver: parentheses around something that binds tightly enough go.
fn postfix_receiver(f: &Formatter<'_>, expr: AnyExpr) -> AnyExpr {
    peel(f, expr, |inner| {
        binds_as_postfix_operand(f, inner) || matches!(inner, AnyExpr::Paren(_))
    })
}

/// A unary operand: as a receiver, and a literal needs none either.
fn unary_operand(f: &Formatter<'_>, expr: AnyExpr) -> AnyExpr {
    peel(f, expr, |inner| {
        binds_as_postfix_operand(f, inner)
            || matches!(inner, AnyExpr::Paren(_) | AnyExpr::Literal(_))
    })
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
/// transparent parentheses collapse into this pair.
impl FormatNode for ParenExpr {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        let d = f.docs;
        let inner = peel(f, f.tree.present(node.inner), |_| true);
        d.group(d.concat([
            f.token(node.l),
            d.indent(d.concat([Doc::SOFT_LINE, f.alt(inner)])),
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
    before: Doc<'a>,
    op: BinaryOpTok,
    rhs: AnyExpr,
    after: Doc<'a>,
}

/// The chain `id` heads: its first operand and the members after it, nested
/// binary expressions in the same row lifted out, transparent parentheses
/// around a left operand in the same row peeled.
fn chain_members<'a>(
    f: &Formatter<'a>,
    id: Id<BinaryExpr>,
    members: &mut Vec<Member<'a>>,
) -> AnyExpr {
    let tree = f.tree;
    let node = tree.get(id);
    let rhs = tree.present(node.rhs);
    let Some(row) = chain_row(node.op.op()) else {
        members.push(Member {
            before: Doc::NIL,
            op: node.op,
            rhs,
            after: Doc::NIL,
        });
        return node.lhs;
    };
    let in_group = |expr: AnyExpr| match expr {
        AnyExpr::Binary(inner) => chain_row(tree.get(inner).op.op()) == Some(row),
        _ => false,
    };
    let lhs = peel(f, node.lhs, in_group);
    let first = match lhs {
        AnyExpr::Binary(left) if in_group(lhs) => {
            let first = chain_members(f, left, members);
            let trailing = docs::trailing(f.docs, tree, f.comments.trailing(AnyNodeId::of(left)));
            if let Some(last) = members.last_mut() {
                last.after = f.docs.concat([last.after, trailing]);
            }
            first
        }
        other => other,
    };
    match rhs {
        AnyExpr::Binary(right) if in_group(rhs) => {
            let before = docs::leading(f.docs, tree, f.comments.leading(AnyNodeId::of(right)));
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
            let trailing = docs::trailing(f.docs, tree, f.comments.trailing(AnyNodeId::of(right)));
            if let Some(last) = members.last_mut() {
                last.after = f.docs.concat([last.after, trailing]);
            }
        }
        rhs => members.push(Member {
            before: Doc::NIL,
            op: node.op,
            rhs,
            after: Doc::NIL,
        }),
    }
    first
}

/// The operator's spelling, with the comments after its tokens.
fn operator<'a>(f: &Formatter<'a>, op: BinaryOpTok) -> Doc<'a> {
    let d = f.docs;
    let mut tokens = op.tokens();
    let first = tokens
        .next()
        .unwrap_or_else(|| unreachable!("an operator has a token"));
    d.concat(
        std::iter::once(f.spelled(op.op().spelling(), first))
            .chain(tokens.map(|token: TokenIdx| f.comments_after(token))),
    )
}

/// `a + b - c`: the first operand on the line, each further `op operand`
/// after it, all on one line or each on a line of its own, indented. The
/// first operand's trailing comments go inside that indentation, with the
/// operators they precede. An operator after an operand that breaks inside
/// braces of its own (`if (c) { … } else { … } - x`) stays on its closing
/// line, as rustfmt keeps it.
impl FormatNode for BinaryExpr {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let d = f.docs;
        let mut members = Vec::new();
        let first = chain_members(f, id, &mut members);
        let first_id = first.node_id();
        let first_doc = f.bare(first_id);
        let mut rest = vec![f.trailing_of(first_id)];
        let mut operands = vec![first_doc];
        let mut previous = (first_id, first_doc);
        for member in members {
            let rhs_id = member.rhs.node_id();
            let rhs = f.alt(member.rhs);
            let line = if braced(previous.0) && previous.1.will_break() {
                Doc::SPACE
            } else {
                Doc::SOFT_LINE_OR_SPACE
            };
            previous = (rhs_id, rhs);
            operands.push(rhs);
            rest.push(d.concat([
                member.before,
                line,
                operator(f, member.op),
                Doc::SPACE,
                rhs,
                member.after,
            ]));
        }
        d.group(d.concat([
            multi_line_before_last(&operands),
            f.leading_of(first_id),
            first_doc,
            d.indent(d.concat(rest)),
        ]))
    }
}

/// `expr is pattern`; after a comment that ends the expression's line, the
/// `is` starts the next one level in.
impl FormatNode for IsExpr {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        let d = f.docs;
        d.concat([
            f.alt(node.expr),
            d.indent(d.concat([Doc::SPACE, f.token(node.is_kw)])),
            Doc::SPACE,
            f.alt(f.tree.present(node.pattern)),
        ])
    }
}

impl FormatNode for UnaryExpr {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        let operand = unary_operand(f, f.tree.present(node.operand));
        f.docs.concat([f.token(node.op), f.alt(operand)])
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
    )
}

/// `key: value`; its comma is the list's.
impl FormatNode for MapEntry {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        f.docs.concat([
            f.alt(node.key),
            f.token(f.tree.present(node.colon)),
            Doc::SPACE,
            f.alt(f.tree.present(node.value)),
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
    )
}

/// `key: value` or the shorthand `key`.
impl FormatNode for ObjectField {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        let key = match node.key {
            AnyObjectKey::Name(name) => f.node(name),
            AnyObjectKey::String(string) => f.node(string),
            AnyObjectKey::Backtick(backtick) => f.node(backtick),
            AnyObjectKey::Bogus(bogus) => f.tree.absent(bogus),
        };
        f.docs.concat([
            key,
            node.value.map_or(Doc::NIL, |value| {
                f.docs.concat([
                    f.token(value.colon),
                    Doc::SPACE,
                    f.alt(f.tree.present(value.value)),
                ])
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
/// — when it may be: a lambda or `spawn` with its body broken, a literal
/// with one element per line, or, as the only argument, a call (rustfmt's
/// overflow of a nested call: `f(g(` on one line, `g`'s arguments broken).
/// A multi-line template needs no hugging: it is an atom measured by its
/// first line, so `f(`…`)` stays as it is. Both layouts are built from one
/// printing of the argument's parts, so nested calls cost no more than
/// their size.
struct ArgForms<'a> {
    flat: Doc<'a>,
    hugged: Option<Doc<'a>>,
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
        other => (f.alt(other), None),
    };
    let label = label(f, arg);
    ArgForms {
        flat: f.with_comments(arg, d.concat([label, flat])),
        hugged: hugged.map(|hugged| f.with_comments(arg, d.concat([label, hugged]))),
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
        let parts = f.delimited_items(node.l.idx, &items, r);
        let hugged = hug(f, node.l.idx, args, &items, &forms, r);
        let choice = match hugged {
            Some(hug) => d.best_fitting([parts, hug, d.group(parts)]),
            None => d.group(parts),
        };
        Self {
            choice,
            broken: d.expanded_group(parts),
            hugged,
        }
    }
}

/// The hug layout of `(args)`: the arguments before the last on one line,
/// the last broken against the closing parenthesis. None when the last may
/// not be hugged, or when something before it would not keep to that one
/// line: an argument sure to break, a comment that ends a line or leads the
/// last argument on a line of its own, a blank line the author left.
fn hug<'a>(
    f: &Formatter<'a>,
    l: TokenIdx,
    args: &[SepItem<Id<Arg>, punct::Comma>],
    items: &[ListItem<'a>],
    forms: &[ArgForms<'a>],
    r: Token<punct::RParen>,
) -> Option<Doc<'a>> {
    let d = f.docs;
    let (last, init) = args.split_last()?;
    let hugged = forms.last()?.hugged?;
    let open = f.comments_inside(l);
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
    {
        return None;
    }
    Some(
        d.concat([
            d.text(f.tree.token_text(l)),
            open,
            d.concat(before),
            hugged,
            last.sep
                .map_or(Doc::NIL, |comma| f.comments_after(comma.idx)),
            f.token(r),
        ]),
    )
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
        d.indent(d.concat([f.comments_inside(l), Doc::SOFT_LINE, f.alt(index)])),
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
    /// `.name`, `?.name`, `<T>`, `.as<T>`, `@spec`, a tagged template: glued
    /// to what it follows. A `field` is a named access, where the expanded
    /// layout may start a line.
    Plain { doc: Doc<'a>, field: bool },
    /// `(args)` or `?.(args)`.
    Call { prefix: Doc<'a>, args: CallArgs<'a> },
    /// `[i]` or `?.[i]`.
    Index { prefix: Doc<'a>, parts: Doc<'a> },
}

impl<'a> Link<'a> {
    fn is_plain(&self) -> bool {
        matches!(self, Self::Plain { .. })
    }

    fn is_field(&self) -> bool {
        matches!(self, Self::Plain { field: true, .. })
    }

    /// The link as it prints on a line. As a `continuation` — on the line
    /// of what it follows, which may end in a line comment — the `.name` or
    /// `?.` that starts the next line then goes one level in; in the
    /// expanded layout the links are indented already.
    fn doc(&self, d: Docs<'a>, continuation: bool) -> Doc<'a> {
        let step = |doc: Doc<'a>| if continuation { d.indent(doc) } else { doc };
        match *self {
            Self::Plain { doc, .. } => step(doc),
            Self::Call { prefix, ref args } => d.concat([step(prefix), args.choice]),
            Self::Index { prefix, parts } => d.concat([step(prefix), d.group(parts)]),
        }
    }
}

/// The receiver a chain starts from and the links after it.
struct Chain<'a> {
    first: Doc<'a>,
    /// The receiver's trailing comments, placed by the layout: inside the
    /// chain's indentation when the chain breaks, so that a comment between
    /// the receiver and its first `.call()` sits with the call.
    first_trailing: Doc<'a>,
    links: Vec<Link<'a>>,
}

impl<'a> Chain<'a> {
    /// The chain `expr` is the end of, built down to its receiver. The
    /// expressions along the spine are not printed as nodes of their own,
    /// so their trailing comments are added here, after the link that
    /// closes each; `expr` itself is printed as a node by its caller.
    fn of(f: &Formatter<'a>, expr: AnyExpr) -> Self {
        let mut chain = Self {
            first: Doc::NIL,
            first_trailing: Doc::NIL,
            links: Vec::new(),
        };
        chain.walk(f, expr, true);
        chain
    }

    /// The receiver: its leading comments and itself; its trailing comments
    /// are kept for the layout.
    fn receiver(&mut self, f: &Formatter<'a>, id: AnyNodeId) {
        self.first = f.docs.concat([f.leading_of(id), f.bare(id)]);
        self.first_trailing = f.trailing_of(id);
    }

    /// `.name`
    fn named<S: baml_syntax::token::TokenSet>(
        f: &Formatter<'a>,
        dot: Token<S>,
        name: Id<baml_syntax::nodes::Name>,
    ) -> Link<'a> {
        Link::Plain {
            doc: f.docs.concat([f.token(dot), f.node(name)]),
            field: true,
        }
    }

    /// A call's callee: walked like any receiver, unless it is a member
    /// access in parentheses the call needs (`(v.f)(3)`, `(t.0)(x)`), which
    /// stays whole, parentheses and all.
    fn callee(&mut self, f: &Formatter<'a>, callee: AnyExpr) {
        if is_grouped_member(f, callee) {
            self.receiver(f, callee.node_id());
        } else {
            self.walk(f, callee, false);
        }
    }

    fn walk(&mut self, f: &Formatter<'a>, expr: AnyExpr, outermost: bool) {
        let tree = f.tree;
        let d = f.docs;
        let expr = postfix_receiver(f, expr);
        match expr {
            AnyExpr::Path(path) => {
                let node = tree.get(path);
                self.receiver(f, AnyNodeId::of(node.head));
                for segment in tree.list(node.segments) {
                    self.links
                        .push(Self::named(f, segment.dot, tree.present(segment.name)));
                }
            }
            AnyExpr::GenericApply(apply) => {
                let node = tree.get(apply);
                self.walk(f, node.base, false);
                self.links.push(Link::Plain {
                    doc: f.node(node.args),
                    field: false,
                });
            }
            AnyExpr::FieldAccess(access) => {
                let node = tree.get(access);
                self.walk(f, node.base, false);
                self.links
                    .push(Self::named(f, node.dot, tree.present(node.name)));
            }
            AnyExpr::OptionalFieldAccess(access) => {
                let node = tree.get(access);
                self.walk(f, node.base, false);
                self.links
                    .push(Self::named(f, node.qdot, tree.present(node.name)));
            }
            AnyExpr::TupleField(access) => {
                let node = tree.get(access);
                self.walk(f, node.base, false);
                self.links.push(Link::Plain {
                    doc: d.concat([f.token(node.dot), f.token(node.slot)]),
                    field: true,
                });
            }
            AnyExpr::Upcast(upcast) => {
                let node = tree.get(upcast);
                self.walk(f, node.base, false);
                self.links.push(Link::Plain {
                    doc: d.concat([
                        f.token(node.dot),
                        f.token(node.as_kw),
                        f.node(tree.present(node.args)),
                    ]),
                    field: true,
                });
            }
            AnyExpr::Spec(spec) => {
                let node = tree.get(spec);
                self.walk(f, node.base, false);
                self.links.push(Link::Plain {
                    // `@` cannot start a line: it tucks in before a pending comment.
                    doc: d.concat([
                        d.text(tree.token_text(node.at.idx)),
                        f.comments_after(node.at.idx),
                        f.token(tree.present(node.selector)),
                    ]),
                    field: false,
                });
            }
            AnyExpr::TaggedTemplate(tagged) => {
                let node = tree.get(tagged);
                self.walk(f, node.tag, false);
                self.links.push(Link::Plain {
                    doc: f.node(node.template),
                    field: false,
                });
            }
            AnyExpr::Call(call) => {
                let node = tree.get(call);
                self.callee(f, node.callee);
                self.links.push(Link::Call {
                    prefix: Doc::NIL,
                    args: CallArgs::of(f, node.args),
                });
            }
            AnyExpr::OptionalCall(call) => {
                let node = tree.get(call);
                self.callee(f, node.callee);
                self.links.push(Link::Call {
                    prefix: f.token(node.qdot),
                    args: CallArgs::of(f, node.args),
                });
            }
            AnyExpr::Index(index) => {
                let node = tree.get(index);
                self.walk(f, node.base, false);
                self.links.push(Link::Index {
                    prefix: Doc::NIL,
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
                self.walk(f, node.base, false);
                self.links.push(Link::Index {
                    prefix: f.token(node.qdot),
                    parts: index_parts(
                        f,
                        node.l.idx,
                        tree.present(node.index),
                        tree.present(node.r).idx,
                    ),
                });
            }
            other => {
                self.receiver(f, other.node_id());
                return;
            }
        }
        if !outermost {
            let trailing = f.trailing_of(expr.node_id());
            match self.links.last_mut() {
                Some(Link::Plain { doc, .. }) => *doc = d.concat([*doc, trailing]),
                Some(Link::Call { .. } | Link::Index { .. }) => self.links.push(Link::Plain {
                    doc: trailing,
                    field: false,
                }),
                None => self.first_trailing = d.concat([self.first_trailing, trailing]),
            }
        }
    }

    /// The chain laid out, `tail` after it on its last line: on one line;
    /// the last call's last argument hugged; every call on its own line when
    /// each line then fits whole; only the last call's (or index's)
    /// brackets broken, the plain accesses after it following; every call
    /// on its own line. A forced break before the last call (a multi-line
    /// lambda, a comment) leaves only the last.
    fn doc(&self, f: &Formatter<'a>, tail: Doc<'a>) -> Doc<'a> {
        let d = f.docs;
        let links = |links: &[Link<'a>]| d.concat(links.iter().map(|link| link.doc(d, true)));
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
            return self.expanded(f, tail);
        }
        let suffix = d.concat([links(suffix), tail]);
        let flat = d.concat([before_last, last.doc(d, true), suffix]);
        let expanded = self.expanded(f, tail);
        let (tail_broken, hug) = match *last {
            Link::Call {
                prefix: qdot,
                ref args,
            } => (
                d.concat([before_last, qdot, args.broken, suffix]),
                args.hugged
                    .map(|hug| d.concat([before_last, qdot, hug, suffix])),
            ),
            Link::Index {
                prefix: qdot,
                parts,
            } => (
                d.concat([before_last, qdot, d.expanded_group(parts), suffix]),
                None,
            ),
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

    /// Every call on a line of its own, indented under the receiver, `tail`
    /// on the last of them. The receiver keeps the calls made directly on
    /// it and the plain accesses up to the first call's name; that first
    /// call stays on the line when it fits; the plain accesses between calls
    /// are glued to the call they precede, those after the last call form a
    /// line of their own. A line can only start with a named access
    /// (`.name`): a call, an index, a generic application, `@spec` or a
    /// tagged template after a line break would not parse as part of the
    /// chain, so a run with no named access before its calls joins the line
    /// before it. The receiver's trailing comments go inside the indentation
    /// with the first call.
    fn expanded(&self, f: &Formatter<'a>, tail: Doc<'a>) -> Doc<'a> {
        let d = f.docs;
        let mut rest: &[Link<'a>] = &self.links;
        // The receiver's trailing comments stay in order: on its line after
        // it when they fit there; when they end the line, nothing is glued
        // to the receiver and the comments go inside the indentation, with
        // the first group, so that what follows them starts a line one
        // level in.
        let trailing_ends_line = self.first_trailing.will_break();
        let mut head = vec![self.first];
        if !trailing_ends_line {
            head.push(self.first_trailing);
        }
        while let Some((link, after)) = rest.split_first() {
            if link.is_plain() {
                break;
            }
            head.push(link.doc(d, false));
            rest = after;
        }
        let plain_run = rest.iter().take_while(|link| link.is_plain()).count();
        let glued = if trailing_ends_line {
            0
        } else if plain_run == rest.len() {
            plain_run
        } else {
            rest[..plain_run]
                .iter()
                .rposition(Link::is_field)
                .unwrap_or(plain_run)
        };
        let inner_trailing = if trailing_ends_line {
            self.first_trailing
        } else {
            Doc::NIL
        };
        head.extend(rest[..glued].iter().map(|link| link.doc(d, false)));
        rest = &rest[glued..];
        let mut groups: Vec<Vec<Doc<'a>>> = Vec::new();
        while !rest.is_empty() {
            let plain = rest.iter().take_while(|link| link.is_plain()).count();
            let callish = rest[plain..]
                .iter()
                .take_while(|link| !link.is_plain())
                .count();
            let (run, after) = rest.split_at(plain + callish);
            rest = after;
            // The run starts a line at its first named access; what comes
            // before that stays on the line before.
            let starts = run[..plain].iter().position(Link::is_field);
            let (joined, group) = run.split_at(starts.unwrap_or(run.len()));
            let joined = joined.iter().map(|link| link.doc(d, false));
            match groups.last_mut() {
                Some(previous) => previous.extend(joined),
                None => head.extend(joined),
            }
            if !group.is_empty() {
                groups.push(group.iter().map(|link| link.doc(d, false)).collect());
            }
        }
        let groups: Vec<Doc<'a>> = groups.into_iter().map(|group| d.concat(group)).collect();
        let mut doc = vec![d.concat(head)];
        match groups.split_first() {
            Some((first_group, later)) => {
                // The first call stays on the receiver's line when it fits
                // there whole; one that is sure to break (a multi-line
                // lambda in its arguments) starts a line like the calls
                // after it.
                let first_line = if first_group.will_break() {
                    Doc::HARD_LINE
                } else {
                    Doc::SOFT_LINE
                };
                doc.push(d.group(d.indent(d.concat([inner_trailing, first_line, *first_group]))));
                doc.push(
                    d.indent(
                        d.concat(
                            later
                                .iter()
                                .map(|group| d.concat([Doc::HARD_LINE, *group]))
                                .chain([tail]),
                        ),
                    ),
                );
            }
            None => doc.extend([inner_trailing, tail]),
        }
        d.concat(doc)
    }
}

/// `expr` with `tail` after it on its last line: for a member chain, inside
/// the chain's indentation when the chain breaks, so that a `catch` after a
/// broken chain hangs off its last call rather than sitting at the indent
/// of the chain's first line; for anything else, simply after it.
pub(super) fn with_tail<'a>(f: &Formatter<'a>, expr: AnyExpr, tail: Doc<'a>) -> Doc<'a> {
    let d = f.docs;
    match expr {
        AnyExpr::Path(_)
        | AnyExpr::GenericApply(_)
        | AnyExpr::FieldAccess(_)
        | AnyExpr::OptionalFieldAccess(_)
        | AnyExpr::Upcast(_)
        | AnyExpr::Spec(_)
        | AnyExpr::TaggedTemplate(_)
        | AnyExpr::Call(_)
        | AnyExpr::OptionalCall(_)
        | AnyExpr::Index(_)
        | AnyExpr::OptionalIndex(_) => {
            let id = expr.node_id();
            d.concat([
                f.leading_of(id),
                Chain::of(f, expr).doc(f, d.concat([f.trailing_of(id), tail])),
            ])
        }
        _ => d.concat([f.alt(expr), tail]),
    }
}

macro_rules! chain_rules {
    ($($node:ident => $variant:ident),* $(,)?) => {
        $(impl FormatNode for $node {
            fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
                Chain::of(f, AnyExpr::$variant(id)).doc(f, Doc::NIL)
            }
        })*
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
