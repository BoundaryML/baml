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
        AnyLiteralTypeValue, ArrayType, AssocTypeBinding, FunctionType, FunctionTypeParam,
        GenericArgs, GenericParam, GenericParamBounds, GenericParamList, LiteralType, OptionalType,
        ParenType, PathType, QualifiedPath, TupleType, TypeArgs, UnionType,
    },
    tree::Id,
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
            f.token(node.l),
            d.indent(d.concat([Doc::SOFT_LINE, f.alt(f.tree.present(node.inner))])),
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
            |ty| f.alt(ty),
        )
    }
}

/// `(params) -> R throws E`, laid out like a signature: the parameters
/// break first, then `throws E` moves under the return type.
impl FormatNode for FunctionType {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        let d = f.docs;
        let tail = d.group(d.concat([
            Doc::SPACE,
            f.token(node.arrow),
            Doc::SPACE,
            f.alt(f.tree.present(node.ret)),
            node.throws.map_or(Doc::NIL, |throws| {
                d.indent(d.concat([Doc::SOFT_LINE_OR_SPACE, f.node(throws)]))
            }),
        ]));
        d.group(d.concat([
            f.delimited_parts(
                node.l,
                f.tree.list(node.params),
                f.tree.present(node.r),
                |param| f.node(param),
            ),
            tail,
        ]))
    }
}

/// `T`, `name: T` or `name?: T`
impl FormatNode for FunctionTypeParam {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        let d = f.docs;
        d.concat([
            node.name.map_or(Doc::NIL, |name| {
                d.concat([
                    f.node(name.name),
                    name.question.map_or(Doc::NIL, |question| f.token(question)),
                    f.token(name.colon),
                    Doc::SPACE,
                ])
            }),
            f.alt(f.tree.present(node.ty)),
        ])
    }
}

/// `T?`
impl FormatNode for OptionalType {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        f.docs.concat([f.alt(node.inner), f.token(node.q)])
    }
}

/// `T[]`
impl FormatNode for ArrayType {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        f.docs.concat([
            f.alt(node.inner),
            f.token(node.l),
            f.token(f.tree.present(node.r)),
        ])
    }
}

/// `A | B | C`; when it does not fit, `A` stays and each `| B` takes a line
/// of its own, indented.
impl FormatNode for UnionType {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        let d = f.docs;
        let arms = f.tree.list(node.arms.as_list()).iter().map(|arm| {
            d.concat([
                Doc::SOFT_LINE_OR_SPACE,
                f.token(arm.pipe),
                Doc::SPACE,
                f.alt(f.tree.present(arm.ty)),
            ])
        });
        d.group(d.concat([f.alt(node.first), d.indent(d.concat(arms))]))
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
            f.alt(f.tree.present(node.first)),
            d.indent(d.concat(rest)),
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
            |arg| f.alt(arg),
        )
    }
}

/// `Item = T`
impl FormatNode for AssocTypeBinding {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        f.docs.concat([
            f.node(node.name),
            Doc::SPACE,
            f.token(node.eq),
            Doc::SPACE,
            f.alt(f.tree.present(node.ty)),
        ])
    }
}
