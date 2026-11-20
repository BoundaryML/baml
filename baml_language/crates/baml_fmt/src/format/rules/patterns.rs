//! Patterns: `let x: int`, `_`, `Point { x, y: let py }`, `[a, ..rest]: T[]`,
//! a type, `(p)`, `(a, b)`, `A | B`. A destructure's fields and an array pattern's
//! elements lay out like an object's and an array's: all on one line, or one
//! per line with a trailing comma.

use baml_syntax::{
    nodes::{
        AnyArrayPatternElement, ArrayPattern, BindingPattern, DestructurePattern, FieldPattern,
        ParenPattern, PatternAscription, RestPattern, TuplePattern, TypePattern, UnionPattern,
        WildcardPattern,
    },
    tree::{AnyNodeId, Id, NodeUnion as _},
};

use crate::{
    format::{FormatNode, Formatter, layout::ListItem},
    ir::Doc,
};

/// `: pattern`; comments between the `:` and the pattern put it one level
/// in, as after a type ascription's `:`.
fn ascription<'a>(f: &Formatter<'a>, ascription: Option<PatternAscription>) -> Doc<'a> {
    ascription.map_or(Doc::NIL, |ascription| {
        f.docs.concat([
            f.token(ascription.colon),
            f.after_operator([ascription.colon.idx], f.tree.present(ascription.pattern)),
        ])
    })
}

/// `let x`, `x`, `let x: int | string`
impl FormatNode for BindingPattern {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        f.docs.concat([
            node.introducer
                .map_or(Doc::NIL, |kw| f.docs.concat([f.token(kw), Doc::SPACE])),
            f.node(node.name),
            ascription(f, node.ascription),
        ])
    }
}

/// `_` or `let _`
impl FormatNode for WildcardPattern {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        f.docs.concat([
            node.introducer
                .map_or(Doc::NIL, |kw| f.docs.concat([f.token(kw), Doc::SPACE])),
            f.token(node.underscore),
        ])
    }
}

/// `let Point<T> { x, y: let py }`
impl FormatNode for DestructurePattern {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        let d = f.docs;
        let items: Vec<ListItem<'a>> = f
            .tree
            .list(node.fields)
            .iter()
            .enumerate()
            .map(|(i, field)| {
                f.list_item(
                    AnyNodeId::of(field.item),
                    field.sep.map(|comma| comma.idx),
                    i == 0,
                )
            })
            .collect();
        d.concat([
            node.introducer
                .map_or(Doc::NIL, |kw| d.concat([f.token(kw), Doc::SPACE])),
            f.node(node.path),
            Doc::SPACE,
            d.group(f.list_parts(
                AnyNodeId::of(id),
                node.l.idx,
                &items,
                f.tree.present(node.r).idx,
                true,
                f.heuristics.object_fields,
            )),
        ])
    }
}

/// `x` or `x: pattern`
impl FormatNode for FieldPattern {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        f.docs.concat([f.node(node.name), ascription(f, node.sub)])
    }
}

/// `[a, ..let rest]: T[]`
impl FormatNode for ArrayPattern {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        let d = f.docs;
        let items: Vec<ListItem<'a>> = f
            .tree
            .list(node.elements)
            .iter()
            .enumerate()
            .map(|(i, element)| {
                let id = match element.item {
                    AnyArrayPatternElement::Pattern(pattern) => pattern.node_id(),
                    AnyArrayPatternElement::Rest(rest) => AnyNodeId::of(rest),
                    AnyArrayPatternElement::Bogus(bogus) => f.tree.absent(bogus),
                };
                f.list_item(id, element.sep.map(|comma| comma.idx), i == 0)
            })
            .collect();
        d.concat([
            d.group(f.list_parts(
                AnyNodeId::of(id),
                node.l.idx,
                &items,
                f.tree.present(node.r).idx,
                false,
                f.heuristics.array_elements,
            )),
            node.ascription
                .map_or(Doc::NIL, |ascription| f.node(ascription)),
        ])
    }
}

/// `..` or `..pattern`
impl FormatNode for RestPattern {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        f.docs.concat([
            f.token(node.dots),
            node.pattern.map_or(Doc::NIL, |pattern| f.alt(pattern)),
        ])
    }
}

impl FormatNode for TypePattern {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        f.alt(f.tree.get(id).ty)
    }
}

/// `(inner)`, the inside on a line of its own when it does not fit.
impl FormatNode for ParenPattern {
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

/// `(a, b)`, or `(a,)` with the comma that tells a one-element tuple pattern
/// from a parenthesised one; one element per line when it does not fit.
impl FormatNode for TuplePattern {
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

/// `A | B | C`; when it does not fit, each `| B` takes a line of its own,
/// one level in under the first alternative — except as a match or catch
/// arm's whole pattern, where the `|` lines sit at the arm's indent, as
/// rustfmt lays out an arm's alternatives (`indent` false).
pub(super) fn union_pattern<'a>(f: &Formatter<'a>, id: Id<UnionPattern>, indent: bool) -> Doc<'a> {
    let node = f.tree.get(id);
    let d = f.docs;
    let arms = d.concat(f.tree.list(node.arms.as_list()).iter().map(|arm| {
        d.concat([
            Doc::SOFT_LINE_OR_SPACE,
            f.token(arm.pipe),
            Doc::SPACE,
            f.alt(f.tree.present(arm.pattern)),
        ])
    }));
    let arms = if indent { d.indent(arms) } else { arms };
    d.group(d.concat([f.alt(node.first), arms]))
}

impl FormatNode for UnionPattern {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        union_pattern(f, id, true)
    }
}
