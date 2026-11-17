//! Attributes: `@name(args)` on a member, `@@name(args)` on a declaration,
//! the arguments all on one line, within the width rustfmt gives an
//! attribute's, or one per line with a trailing comma. A multi-line backtick
//! string is an atom measured by its first line, so a `@description(…)`
//! holding one stays on its line, the template's lines under it.

use baml_syntax::{
    nodes::{AttrArgs, Attribute, BlockAttr, DottedName},
    token::{Token, TokenSet},
    tree::{Id, Missing, NodeUnion as _},
};

use crate::{
    format::{FormatNode, Formatter},
    ir::Doc,
};

/// `@name(args)` or `@@name(args)`.
fn attribute<'a, S: TokenSet>(
    f: &Formatter<'a>,
    at: Token<S>,
    path: Result<Id<DottedName>, Missing>,
    args: Option<Id<AttrArgs>>,
) -> Doc<'a> {
    f.docs.concat([
        f.token(at),
        f.node(f.tree.present(path)),
        args.map_or(Doc::NIL, |args| f.node(args)),
    ])
}

impl FormatNode for Attribute {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        attribute(f, node.at, node.path, node.args)
    }
}

impl FormatNode for BlockAttr {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        attribute(f, node.at, node.path, node.args)
    }
}

/// `a.b.c`
impl FormatNode for DottedName {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        let d = f.docs;
        d.concat(std::iter::once(f.node(node.head)).chain(
            f.tree.list(node.segments).iter().map(|segment| {
                d.concat([f.token(segment.dot), f.node(f.tree.present(segment.name))])
            }),
        ))
    }
}

impl FormatNode for AttrArgs {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        f.delimited(
            node.l,
            f.tree.list(node.args),
            f.tree.present(node.r),
            Some(f.heuristics.attr_args),
            |arg| arg.node_id(),
            |arg| f.alt(arg),
        )
    }
}
