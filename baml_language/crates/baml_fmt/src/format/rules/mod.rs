//! The rule for each node type. A node type listed under [`verbatim`] has no
//! layout of its own yet and prints as written; as rules are written, their
//! types move out of that list (a type in both places, or in neither, does
//! not compile).

use baml_syntax::{
    nodes::{self, AnyItem, Bogus, SourceFile},
    tree::{Id, NodeUnion as _},
};

use super::{FormatNode, Formatter};
use crate::{comments::docs, ir::Doc};

mod attrs;
pub(in crate::format) mod control;
mod exprs;
mod items;
mod patterns;
mod stmts;
mod templates;
mod types;

/// Items are separated by one blank line, except that a run of globals keeps
/// the grouping it was written with: adjacent `let`s stay adjacent, and a
/// blank line the author put between two of them stays. The blank line goes
/// where the author put one among the comments before an item: a comment
/// written right after the previous item, with the blank line after it,
/// stays with that item. Comments before the first item come first; a file
/// with nothing but comments is those comments.
impl FormatNode for SourceFile {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let d = f.docs;
        let items = f.tree.list(f.tree.get(id).items);
        let mut parts = vec![docs::dangling(
            d,
            f.tree,
            f.comments.dangling(f.tree.root_id()),
            true,
        )];
        for (index, &item) in items.iter().enumerate() {
            if index > 0 {
                let id = item.node_id();
                let blank_before = f.comments.gap_before_node(f.tree, id).has_blank_line();
                let grouped_globals = matches!(item, AnyItem::Let(_))
                    && matches!(items[index - 1], AnyItem::Let(_))
                    && !blank_before;
                let comments_hug_previous = !blank_before
                    && f.comments
                        .leading(id)
                        .iter()
                        .any(|comment| comment.after.has_blank_line());
                parts.push(if grouped_globals || comments_hug_previous {
                    Doc::HARD_LINE
                } else {
                    Doc::EMPTY_LINE
                });
            }
            parts.push(f.alt(item));
        }
        parts.push(Doc::HARD_LINE);
        d.concat(parts)
    }
}

impl FormatNode for Bogus {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        f.tree.absent(id)
    }
}

/// The node types that print as written until their rule exists.
macro_rules! verbatim {
    ($($node:ident),* $(,)?) => {
        $(impl FormatNode for nodes::$node {
            fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
                f.verbatim(id)
            }
        })*
    };
}

verbatim! {
    // items
    RemovedItem, JinjaArg,
    // the parts of a template, printed by the template as a whole
    TemplateInterp, TemplateFor, TemplateForOpen, TemplateIf, TemplateIfOpen, TemplateElif,
    TemplateElse, TemplateEndTag,
}
