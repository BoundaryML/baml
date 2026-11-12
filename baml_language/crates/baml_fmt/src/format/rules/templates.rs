//! Backtick strings. One on a single line prints as written. One spanning
//! lines is laid out so that its content sits one level past the line it
//! starts on and its closing backticks close that level, the lines keeping
//! their indentation relative to one another — when that cannot change its
//! value: no block tag, no interpolation spanning lines, and the dedented
//! text (BEP-049 §12, then escapes) the same before and after. Otherwise it
//! prints as written.

use baml_base::{dedent::dedent_backtick, escape::unescape_backtick_string_literal};
use baml_syntax::{
    nodes::{AnyTemplatePart, BacktickString},
    tree::{AnyNodeId, Id},
};

use crate::{
    format::{FormatNode, Formatter},
    ir::Doc,
};

impl FormatNode for BacktickString {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let text = f.node_text(AnyNodeId::of(id));
        if !text.contains('\n') || !reindent_is_safe(f, id) {
            return f.verbatim(id);
        }
        let Some((ticks, content)) = reindented(text) else {
            return f.verbatim(id);
        };
        let d = f.docs;
        // One level past the line the literal starts on for the content; the
        // closing backticks at that line's level. One literal, so that both
        // are relative to that line.
        let level = " ".repeat(f.indent_width);
        let content: String = content
            .lines()
            .map(|line| {
                if line.is_empty() {
                    String::new()
                } else {
                    format!("{level}{line}")
                }
            })
            .collect::<Vec<_>>()
            .join(
                "
",
            );
        d.concat([
            d.text(ticks),
            d.literal(&format!(
                "
{content}
{ticks}"
            )),
        ])
    }
}

/// Whether moving the literal's lines cannot change its value: no block
/// tag, and no interpolation spanning lines.
fn reindent_is_safe(f: &Formatter<'_>, id: Id<BacktickString>) -> bool {
    let tree = f.tree;
    !tree
        .list(tree.get(id).parts)
        .iter()
        .any(|part| match *part {
            AnyTemplatePart::For(_) | AnyTemplatePart::If(_) => true,
            AnyTemplatePart::Interp(interp) => f.node_text(AnyNodeId::of(interp)).contains('\n'),
            AnyTemplatePart::Chunk(_) => false,
            AnyTemplatePart::Bogus(bogus) => f.tree.absent(bogus),
        })
}

/// The literal's backticks and its content dedented (BEP-049 §12), the
/// lines keeping their indentation relative to one another, or `None` when
/// a literal laid out from that content would not have the same value.
fn reindented(text: &str) -> Option<(&str, String)> {
    let ticks = text.bytes().take_while(|&byte| byte == b'`').count();
    if ticks == 0 || text.len() < ticks * 2 {
        return None;
    }
    let inner = &text[ticks..text.len() - ticks];
    let content = dedent_backtick(inner);
    if content.is_empty() {
        return None;
    }
    let value = |inner: &str| unescape_backtick_string_literal(&dedent_backtick(inner));
    if value(inner) != value(&format!("\n{content}\n")) {
        return None;
    }
    Some((&text[..ticks], content))
}
