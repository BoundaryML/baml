//! Types and the angle-bracketed lists on both sides of a generic: `int`,
//! `map<K, V>`, `T?`, `T[]`, `(A | B)`, `(A, B)`, `(a: A) -> R throws E`, `A | B`,
//! `(Base as I).item`, `<T extends A & B>`, `<T, Item = U>`.
//!
//! A union that does not fit keeps its first member where it is and puts
//! each further member on a line of its own under it, led by its `|`; a
//! bracketed list breaks one item per line with a trailing comma, like a
//! parameter list.

use baml_syntax::{
    nodes::{
        AnyLiteralTypeValue, AnyType, ArrayType, AssocTypeBinding, FunctionType, FunctionTypeParam,
        GenericArgs, GenericParam, GenericParamBounds, GenericParamList, LiteralType, OptionalType,
        ParenType, PathType, QualifiedPath, TupleType, TypeArgs, UnionType,
    },
    token::TokenIdx,
    tree::{AnyNodeId, Id, NodeUnion as _},
};

use crate::{
    format::{FormatNode, Formatter},
    ir::Doc,
};

/// `head.segment<args>`
impl FormatNode for PathType {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        let d = f.docs;
        d.concat(
            std::iter::once(f.node(node.head))
                .chain(f.tree.list(node.segments).iter().map(|segment| {
                    d.concat([f.token(segment.dot), f.node(f.tree.present(segment.name))])
                }))
                .chain(node.args.map(|args| f.node(args))),
        )
    }
}

/// `"a"`, `-1`, `10n`, or a backtick string, as written.
impl FormatNode for LiteralType {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        let value = match node.value {
            AnyLiteralTypeValue::Scalar(token) => f.token(token),
            AnyLiteralTypeValue::Backtick(backtick) => f.node(backtick),
            AnyLiteralTypeValue::Bogus(bogus) => f.tree.absent(bogus),
        };
        f.docs
            .concat([node.minus.map_or(Doc::NIL, |minus| f.token(minus)), value])
    }
}

/// `(inner)`, the inner type on a line of its own when it does not fit.
impl FormatNode for ParenType {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        let d = f.docs;
        d.group(d.concat([
            Doc::LINE_SUFFIX_BOUNDARY,
            d.text("("),
            f.comments_inside(node.l.idx),
            d.indent(d.concat([
                f.line_after_opener(node.l.idx),
                f.alt(f.tree.present(node.inner)),
            ])),
            Doc::SOFT_LINE,
            f.token(f.tree.present(node.r)),
        ]))
    }
}

/// `(A, B)`, or `(A,)` with the comma that tells a one-element tuple from a
/// parenthesised type; one element per line with a trailing comma when it
/// does not fit, like a parameter list.
impl FormatNode for TupleType {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        f.tuple(
            node.l,
            f.tree.list(node.elements),
            f.tree.present(node.r),
            |ty| ty.node_id(),
            |ty| f.alt(ty),
        )
    }
}

/// `(params) -> R throws E`, laid out as a function's signature is: `throws
/// E` moves under the return type first, then the parameters break (see
/// `signature` in the declaration rules).
impl FormatNode for FunctionType {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        let d = f.docs;
        let params = f.delimited_parts(
            node.l,
            f.tree.list(node.params),
            f.tree.present(node.r),
            None,
            AnyNodeId::of,
            |param| f.node(param),
        );
        let ret = d.concat([
            f.token(node.arrow),
            Doc::SPACE,
            f.alt(f.tree.present(node.ret)),
        ]);
        super::items::signature(f, params, Doc::NIL, ret, Doc::NIL, node.throws).0
    }
}

/// `T`, `name: T` or `name?: T`
impl FormatNode for FunctionTypeParam {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        let d = f.docs;
        let ty = f.tree.present(node.ty);
        match node.name {
            Some(name) => d.concat([
                f.node(name.name),
                name.question.map_or(Doc::NIL, |question| f.token(question)),
                f.token(name.colon),
                f.after_operator([name.colon.idx], ty),
            ]),
            None => f.alt(ty),
        }
    }
}

/// `T?` and `T[]` — neither `?` nor a bracket can start a line, so a suffix
/// is text glued to its type, and the comments written between the type and
/// the suffix, or inside the brackets, follow it: `T[] // c`. Suffixes on a
/// suffix (`T[]?`) are printed by the outermost as one run, their comments
/// after the last of them, so that a comment is carried past all of them
/// on the first pass rather than past one more on each.
fn suffixed<'a>(f: &Formatter<'a>, ty: AnyType) -> Doc<'a> {
    let d = f.docs;
    // From the outermost suffix inward: the type the suffix applies to and
    // the suffix's tokens.
    let mut layers: Vec<(AnyNodeId, TokenIdx, Option<TokenIdx>)> = Vec::new();
    let mut inner = ty;
    loop {
        inner = match inner {
            AnyType::Optional(id) => {
                let node = f.tree.get(id);
                layers.push((node.inner.node_id(), node.q.idx, None));
                node.inner
            }
            AnyType::Array(id) => {
                let node = f.tree.get(id);
                let r = f.tree.present(node.r);
                layers.push((node.inner.node_id(), node.l.idx, Some(r.idx)));
                node.inner
            }
            AnyType::Path(_)
            | AnyType::Literal(_)
            | AnyType::Paren(_)
            | AnyType::Tuple(_)
            | AnyType::Function(_)
            | AnyType::Union(_)
            | AnyType::Qualified(_)
            | AnyType::Bogus(_) => break,
        };
    }
    debug_assert!(!layers.is_empty(), "a suffix type has a suffix");
    let base = inner.node_id();
    let suffixes = layers
        .iter()
        .rev()
        .flat_map(|&(_, first, second)| std::iter::once(first).chain(second))
        .map(|token| d.text(f.tree.token_text(token)));
    let carried = layers.iter().rev().flat_map(|&(inner, first, second)| {
        f.comments
            .trailing(inner)
            .iter()
            .chain(f.dangling_after(first))
            .chain(second.into_iter().flat_map(|token| f.dangling_after(token)))
            .copied()
    });
    d.concat(
        layers
            .iter()
            .map(|&(inner, ..)| f.leading_of(inner))
            .chain([f.bare(base)])
            .chain(suffixes)
            .chain([f.comments_past(carried)]),
    )
}

impl FormatNode for OptionalType {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        suffixed(f, AnyType::Optional(id))
    }
}

impl FormatNode for ArrayType {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        suffixed(f, AnyType::Array(id))
    }
}

/// `A | B | C`; when it does not fit, `A` stays and each `| B` takes a line
/// of its own, indented. The first member's trailing comments go inside
/// that indentation, with the `|` lines they precede.
impl FormatNode for UnionType {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        let d = f.docs;
        let first = node.first.node_id();
        let arms = f.tree.list(node.arms.as_list()).iter().map(|arm| {
            d.concat([
                Doc::SOFT_LINE_OR_SPACE,
                f.token(arm.pipe),
                Doc::SPACE,
                f.alt(f.tree.present(arm.ty)),
            ])
        });
        d.group(d.concat([
            f.leading_of(first),
            f.bare(first),
            d.indent(d.concat(std::iter::once(f.trailing_of(first)).chain(arms))),
        ]))
    }
}

/// `(Base as Interface).member`
impl FormatNode for QualifiedPath {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        f.docs.concat([
            f.token(node.l),
            f.alt(node.base),
            Doc::SPACE,
            f.token(node.as_kw),
            Doc::SPACE,
            f.alt(f.tree.present(node.iface)),
            f.token(f.tree.present(node.r)),
            f.token(f.tree.present(node.dot)),
            f.node(f.tree.present(node.member)),
        ])
    }
}

/// Declaration-site `<T, U extends I>`
impl FormatNode for GenericParamList {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        f.delimited(
            node.l,
            f.tree.list(node.params),
            f.tree.present(node.r),
            None,
            AnyNodeId::of,
            |param| f.node(param),
        )
    }
}

impl FormatNode for GenericParam {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        f.docs.concat([
            f.node(node.name),
            node.bounds.map_or(Doc::NIL, |bounds| {
                f.docs.concat([Doc::SPACE, f.node(bounds)])
            }),
        ])
    }
}

/// `extends A & B`; when it does not fit, each `& B` takes a line of its
/// own, indented, like a union's arms.
impl FormatNode for GenericParamBounds {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        let d = f.docs;
        let first = f.tree.present(node.first).node_id();
        let rest = f.tree.list(node.rest).iter().map(|arm| {
            d.concat([
                Doc::SOFT_LINE_OR_SPACE,
                f.token(arm.and),
                Doc::SPACE,
                f.alt(f.tree.present(arm.ty)),
            ])
        });
        d.group(d.concat([
            f.token(node.kw),
            Doc::SPACE,
            f.leading_of(first),
            f.bare(first),
            d.indent(d.concat(std::iter::once(f.trailing_of(first)).chain(rest))),
        ]))
    }
}

/// Expression-side `<T, U>`
impl FormatNode for GenericArgs {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        f.delimited(
            node.l,
            f.tree.list(node.args),
            f.tree.present(node.r),
            None,
            |arg| arg.node_id(),
            |arg| f.alt(arg),
        )
    }
}

/// Type-side `<T, Item = U>`
impl FormatNode for TypeArgs {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        f.delimited(
            node.l,
            f.tree.list(node.args),
            f.tree.present(node.r),
            None,
            |arg| arg.node_id(),
            |arg| f.alt(arg),
        )
    }
}

/// `Item = T`; after a comment that ends the name's line, the `=` starts
/// the next one level in.
impl FormatNode for AssocTypeBinding {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        f.docs.concat([
            f.node(node.name),
            f.continuation(
                AnyNodeId::of(node.name),
                f.docs.concat([
                    Doc::SPACE,
                    f.token(node.eq),
                    f.after_operator([node.eq.idx], f.tree.present(node.ty)),
                ]),
            ),
        ])
    }
}
