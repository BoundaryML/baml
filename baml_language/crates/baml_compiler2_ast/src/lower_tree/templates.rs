//! Backtick templates. The text goes through the BEP-049 pipeline the rowan
//! lowering applies — whole-literal dedent (§12) with the non-text parts held
//! out, block-tag whitespace control (§13), then escape decoding — and the
//! structural parts become [`TemplateSegment`]s; the two desugarings
//! (`elaborate_default_segments`, `elaborate_tagged_body`) are
//! `LoweringContext`'s and run on the segments unchanged.
//!
//! The text rules see the literal flat: text, block tags and interpolations
//! in order, with nesting forgotten. Both the flat read and the segment walk
//! visit the parts in the same order, so the text that lands between two
//! consecutive non-text parts is handed to the segment walk one gap at a
//! time (`Texts`).

use baml_base::{dedent::dedent_backtick, escape::unescape_backtick_string_literal};
use baml_syntax::{
    nodes::{
        AnyForHeader, AnyForInit, AnyTemplatePart, BacktickString, ForInHeader, TaggedTemplateExpr,
        TemplateElif, TemplateFor, TemplateIf, TemplateInterp,
    },
    tree::{Id, List},
};
use text_size::{TextRange, TextSize};

use super::{
    View,
    body::{
        block_body, expr_or_missing, expression, let_stmt, pattern, step_stmt,
        warn_const_introducer,
    },
};
use crate::{
    LoweringDiagnostic,
    ast::{Expr, ExprId, Pattern, Stmt, TemplateIfBranch, TemplateSegment, TemplateTag},
    lower_expr_body::LoweringContext,
};

/// An untagged `` `…` `` (BEP §11).
pub(super) fn backtick_expr(
    ctx: &mut LoweringContext,
    v: View<'_>,
    id: Id<BacktickString>,
) -> ExprId {
    let span = v.range_of(id);
    let segments = template_segments(ctx, v, id);
    // The realization is compiler-generated: its nodes are synthetic, the
    // segments' are not.
    let prev_synth = std::mem::replace(&mut ctx.synthesizing, true);
    let elaborated = ctx.elaborate_default_segments(&segments, span);
    ctx.synthesizing = prev_synth;
    ctx.alloc_expr(
        Expr::Template {
            tag: TemplateTag::Default { elaborated },
            segments,
        },
        span,
    )
}

/// A tagged `` tag`…` `` (BEP §10).
pub(super) fn tagged_template_expr(
    ctx: &mut LoweringContext,
    v: View<'_>,
    id: Id<TaggedTemplateExpr>,
) -> ExprId {
    let node = *v.node(id);
    let span = v.range_of(id);
    let tag = expression(ctx, v, node.tag);
    // A bare `prompt` tag is the stdlib `ai.prompt` (BEP §10's examples use
    // the unqualified form and BAML has no prelude). Same `ExprId`, so the
    // span stays the written one.
    if let Expr::Path(path) = &ctx.exprs[tag]
        && let [name] = path.segments()
        && name.as_str() == "prompt"
    {
        ctx.exprs[tag] = Expr::Path(baml_base::compiler_path!(ai.prompt));
    }
    let segments = template_segments(ctx, v, node.template);
    let prev_synth = std::mem::replace(&mut ctx.synthesizing, true);
    let body = ctx.elaborate_tagged_body(&segments, span);
    ctx.synthesizing = prev_synth;
    ctx.alloc_expr(
        Expr::Template {
            tag: TemplateTag::Custom { tag, body },
            segments,
        },
        span,
    )
}

fn template_segments(
    ctx: &mut LoweringContext,
    v: View<'_>,
    id: Id<BacktickString>,
) -> Vec<TemplateSegment> {
    let parts = v.node(id).parts;
    let mut texts = Texts::read(v, parts);
    let mut segments = segments(ctx, v, parts, &mut texts);
    texts.drain_into(&mut segments);
    debug_assert!(
        texts.gaps.next().is_none(),
        "the text read and the segment walk visit the same parts"
    );
    segments
}

// ── Segments ─────────────────────────────────────────────────────────────────

/// The segments of one parts list. The text before each non-text part is
/// taken from `texts`; the text after the last one belongs to whoever closes
/// the list, which knows whether a tag follows it.
fn segments(
    ctx: &mut LoweringContext,
    v: View<'_>,
    parts: List<AnyTemplatePart>,
    texts: &mut Texts,
) -> Vec<TemplateSegment> {
    let mut out = Vec::new();
    for &part in v.tree.list(parts) {
        match part {
            AnyTemplatePart::Chunk(_) => {}
            AnyTemplatePart::Interp(id) => {
                texts.drain_into(&mut out);
                out.push(interpolation(ctx, v, id));
            }
            AnyTemplatePart::For(id) => {
                texts.drain_into(&mut out);
                out.push(template_for(ctx, v, id, texts));
            }
            AnyTemplatePart::If(id) => {
                texts.drain_into(&mut out);
                out.push(template_if(ctx, v, id, texts));
            }
            // A stray tag was reported; it contributes nothing.
            AnyTemplatePart::Bogus(_) => texts.drain_into(&mut out),
        }
    }
    out
}

/// `${…}`: the inner block, raw. An interpolation with no value runs for
/// its effects in the template's own scope.
fn interpolation(
    ctx: &mut LoweringContext,
    v: View<'_>,
    id: Id<TemplateInterp>,
) -> TemplateSegment {
    let node = *v.node(id);
    let span = v.range_of(id);
    if v.tree.list(node.stmts).is_empty() && node.tail.is_none() {
        ctx.diags
            .push(LoweringDiagnostic::EmptyInterpolation { span });
    }
    // The block is `{ … }`: its brace is the `${` token's second byte.
    let block_span = TextRange::new(span.start() + TextSize::from(1), span.end());
    let block = block_body(ctx, v, node.stmts, node.tail, block_span);
    match ctx.unit_interp_stmts(block, span) {
        Some(stmts) => TemplateSegment::Effect { block, stmts },
        None => TemplateSegment::Interp(block),
    }
}

/// `${for …}…${endfor}`, in either header form. A header the parser could
/// not read leaves the same holes the host `for` does.
fn template_for(
    ctx: &mut LoweringContext,
    v: View<'_>,
    id: Id<TemplateFor>,
    texts: &mut Texts,
) -> TemplateSegment {
    let node = *v.node(id);
    let open = *v.node(node.open);
    let open_span = v.range_of(node.open);
    match open.header {
        Ok(AnyForHeader::CStyle(header)) => {
            let header = *v.node(header);
            let init = match header.init {
                Some(AnyForInit::Let(init)) => let_stmt(ctx, v, init),
                Some(AnyForInit::Expr(_) | AnyForInit::Bogus(_)) | None => {
                    ctx.alloc_stmt(Stmt::Missing, open_span)
                }
            };
            let cond = match header.cond {
                Some(cond) => expression(ctx, v, cond),
                None => ctx.alloc_expr(Expr::Missing, open_span),
            };
            let step = header.step.map(|step| step_stmt(ctx, v, step));
            let body = tag_body(ctx, v, node.body, texts);
            TemplateSegment::CStyleFor {
                init,
                cond,
                step,
                body,
            }
        }
        Ok(AnyForHeader::In(header)) => {
            iterator_for(ctx, v, node, Some(*v.node(header)), open_span, texts)
        }
        Ok(AnyForHeader::ParenIn(paren)) => {
            let header = v.node(v.node(paren).header);
            iterator_for(ctx, v, node, Some(*header), open_span, texts)
        }
        Ok(AnyForHeader::Bogus(_)) | Err(_) => iterator_for(ctx, v, node, None, open_span, texts),
    }
}

/// The iterator form, `${for let p in c}`. The collection is lowered before
/// the pattern, as the rowan lowering orders them.
fn iterator_for(
    ctx: &mut LoweringContext,
    v: View<'_>,
    node: TemplateFor,
    header: Option<ForInHeader>,
    open_span: TextRange,
    texts: &mut Texts,
) -> TemplateSegment {
    let collection = header
        .and_then(|header| header.iter.ok())
        .map(|iter| expression(ctx, v, iter));
    let binding = header.and_then(|header| {
        warn_const_introducer(ctx, v, header.introducer);
        header
            .pattern
            .ok()
            .map(|pattern| self::pattern(ctx, v, pattern))
    });
    let collection = collection.unwrap_or_else(|| ctx.alloc_expr(Expr::Missing, open_span));
    let binding = binding.unwrap_or_else(|| ctx.alloc_pattern(Pattern::Wildcard, open_span));
    let body = tag_body(ctx, v, node.body, texts);
    TemplateSegment::For {
        binding,
        collection,
        body,
    }
}

/// `${if c}…${else if c}…${else}…${endif}`: each condition, then its body.
fn template_if(
    ctx: &mut LoweringContext,
    v: View<'_>,
    id: Id<TemplateIf>,
    texts: &mut Texts,
) -> TemplateSegment {
    let node = *v.node(id);
    let open = *v.node(node.open);
    let elifs = v.tree.list(node.elifs);
    let mut branches = Vec::with_capacity(1 + elifs.len());
    let condition = expr_or_missing(ctx, v, open.cond, v.range_of(node.open));
    let body = tag_body(ctx, v, node.then_parts, texts);
    branches.push(TemplateIfBranch { condition, body });
    for &elif in elifs {
        let elif_node = *v.node(elif);
        let condition = expr_or_missing(ctx, v, elif_node.cond, elif_head_span(v, elif_node));
        let body = tag_body(ctx, v, elif_node.parts, texts);
        branches.push(TemplateIfBranch { condition, body });
    }
    let else_body = node
        .else_part
        .map(|else_part| tag_body(ctx, v, v.node(else_part).parts, texts));
    TemplateSegment::If {
        branches,
        else_body,
    }
}

/// The parts between two block tags, trailing text included.
fn tag_body(
    ctx: &mut LoweringContext,
    v: View<'_>,
    parts: List<AnyTemplatePart>,
    texts: &mut Texts,
) -> Vec<TemplateSegment> {
    let mut body = segments(ctx, v, parts, texts);
    texts.drain_into(&mut body);
    body
}

/// `${else if c}` alone, without the parts that follow it in the node.
fn elif_head_span(v: View<'_>, node: TemplateElif) -> TextRange {
    let start = v.token_range(node.open.idx).start();
    let end = match (node.close, node.cond) {
        (Ok(close), _) => v.token_range(close.idx).end(),
        (Err(_), Ok(cond)) => v.range_any(&cond).end(),
        (Err(_), Err(_)) => v.token_range(node.if_kw.idx).end(),
    };
    TextRange::new(start, end)
}

// ── Text ─────────────────────────────────────────────────────────────────────

/// The literal read flat, as the text rules see it.
enum Flat {
    Text(String),
    /// A block tag: `${for …}`, `${if …}`, `${else if …}`, `${else}`,
    /// `${endfor}`, `${endif}`, or a stray one.
    Tag,
    /// A `${…}` interpolation.
    Interp,
}

/// The decoded text of a template, one gap at a time: the text before each
/// non-text part in order, then the text after the last one.
struct Texts {
    gaps: std::vec::IntoIter<Option<String>>,
}

impl Texts {
    fn read(v: View<'_>, parts: List<AnyTemplatePart>) -> Self {
        let mut flat = Vec::new();
        flatten(v, parts, &mut flat);
        dedent(&mut flat);
        strip_around_block_tags(&mut flat);
        let mut gaps = Vec::new();
        let mut current: Option<String> = None;
        for part in flat {
            match part {
                Flat::Text(text) => current
                    .get_or_insert_default()
                    .push_str(&unescape_backtick_string_literal(&text)),
                Flat::Tag | Flat::Interp => gaps.push(current.take()),
            }
        }
        gaps.push(current);
        Self {
            gaps: gaps.into_iter(),
        }
    }

    /// Append the next gap's text, if there is any, as a segment.
    fn drain_into(&mut self, out: &mut Vec<TemplateSegment>) {
        let gap = self.gaps.next();
        debug_assert!(
            gap.is_some(),
            "the text read and the segment walk visit the same parts"
        );
        if let Some(text) = gap.flatten()
            && !text.is_empty()
        {
            out.push(TemplateSegment::Text(text));
        }
    }
}

/// The parts in order with nesting forgotten; adjacent text joined. Every
/// tag is a part, a missing closing tag included, so the two walks agree.
fn flatten(v: View<'_>, parts: List<AnyTemplatePart>, out: &mut Vec<Flat>) {
    for &part in v.tree.list(parts) {
        match part {
            AnyTemplatePart::Chunk(token) => push_text(out, v.text(token.idx)),
            AnyTemplatePart::Interp(_) => out.push(Flat::Interp),
            AnyTemplatePart::For(id) => {
                let node = v.node(id);
                out.push(Flat::Tag);
                flatten(v, node.body, out);
                out.push(Flat::Tag);
            }
            AnyTemplatePart::If(id) => {
                let node = v.node(id);
                out.push(Flat::Tag);
                flatten(v, node.then_parts, out);
                for &elif in v.tree.list(node.elifs) {
                    out.push(Flat::Tag);
                    flatten(v, v.node(elif).parts, out);
                }
                if let Some(else_part) = node.else_part {
                    out.push(Flat::Tag);
                    flatten(v, v.node(else_part).parts, out);
                }
                out.push(Flat::Tag);
            }
            AnyTemplatePart::Bogus(_) => out.push(Flat::Tag),
        }
    }
}

fn push_text(out: &mut Vec<Flat>, text: &str) {
    if text.is_empty() {
        return;
    }
    match out.last_mut() {
        Some(Flat::Text(last)) => last.push_str(text),
        Some(Flat::Tag | Flat::Interp) | None => out.push(Flat::Text(text.to_string())),
    }
}

/// BEP §12: dedent the literal as a whole, with each non-text part standing
/// in as one placeholder character so that whitespace inside `${…}` is
/// never counted (§12 rule 8). A literal on one line is left alone.
fn dedent(flat: &mut Vec<Flat>) {
    let multiline = flat
        .iter()
        .any(|part| matches!(part, Flat::Text(text) if text.contains('\n') || text.contains('\r')));
    if !multiline {
        return;
    }
    let content: String = flat
        .iter()
        .filter_map(|part| match part {
            Flat::Text(text) => Some(text.as_str()),
            Flat::Tag | Flat::Interp => None,
        })
        .collect();
    let placeholder = (0xE000u32..=0xF8FF)
        .filter_map(char::from_u32)
        .find(|c| !content.contains(*c))
        .unwrap_or('\u{F8FF}');
    let mut joined = String::new();
    let mut markers = Vec::new();
    for part in std::mem::take(flat) {
        match part {
            Flat::Text(text) => joined.push_str(&text),
            marker @ (Flat::Tag | Flat::Interp) => {
                joined.push(placeholder);
                markers.push(marker);
            }
        }
    }
    let dedented = dedent_backtick(&joined);
    let mut markers = markers.into_iter();
    for (i, piece) in dedented.split(placeholder).enumerate() {
        if i > 0
            && let Some(marker) = markers.next()
        {
            flat.push(marker);
        }
        if !piece.is_empty() {
            flat.push(Flat::Text(piece.to_string()));
        }
    }
}

/// How much to cut off a text part: `(index, bytes)` at the edge next to
/// the tag.
type Strip = Option<(usize, usize)>;

/// BEP §13: a block tag alone on its source line takes the line with it —
/// the whitespace before it and the whitespace and line break after it.
/// Inline `${…}` interpolations keep their surroundings; one on the tag's
/// line means the tag is not alone.
fn strip_around_block_tags(flat: &mut Vec<Flat>) {
    let plans: Vec<(Strip, Strip)> = (0..flat.len())
        .filter(|&i| matches!(flat[i], Flat::Tag))
        .filter_map(|i| alone_on_line(flat, i))
        .collect();
    // Two tags on consecutive lines share the text between them: applied
    // last to first, each strip's byte count still refers to the edge it
    // measured.
    for (before, after) in plans.into_iter().rev() {
        if let Some((index, bytes)) = before
            && let Some(Flat::Text(text)) = flat.get_mut(index)
        {
            text.truncate(text.len().saturating_sub(bytes));
        }
        if let Some((index, bytes)) = after
            && let Some(Flat::Text(text)) = flat.get_mut(index)
        {
            text.drain(..bytes.min(text.len()));
        }
    }
    flat.retain(|part| !matches!(part, Flat::Text(text) if text.is_empty()));
}

/// The strips for the tag at `tag`, or `None` when something other than
/// blanks shares its line.
fn alone_on_line(flat: &[Flat], tag: usize) -> Option<(Strip, Strip)> {
    let blank = |c: char| c == ' ' || c == '\t';
    // Backwards to the previous line break, or the start of the literal.
    let mut before: Strip = None;
    for (j, part) in flat[..tag].iter().enumerate().rev() {
        let Flat::Text(text) = part else {
            return None;
        };
        if let Some(newline) = text.rfind('\n') {
            let tail = &text[newline + 1..];
            if !tail.chars().all(blank) {
                return None;
            }
            if !tail.is_empty() {
                before = Some((j, tail.len()));
            }
            break;
        }
        if !text.chars().all(blank) {
            return None;
        }
        if !text.is_empty() {
            before = Some((j, text.len()));
        }
    }
    // Forwards to the next line break, which is stripped too, or the end.
    let mut after: Strip = None;
    for (j, part) in flat.iter().enumerate().skip(tag + 1) {
        let Flat::Text(text) = part else {
            return None;
        };
        if let Some(newline) = text.find('\n') {
            if !text[..newline].chars().all(blank) {
                return None;
            }
            after = Some((j, newline + 1));
            break;
        }
        if !text.chars().all(blank) {
            return None;
        }
        if !text.is_empty() {
            after = Some((j, text.len()));
        }
    }
    Some((before, after))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(s: &str) -> Flat {
        Flat::Text(s.to_string())
    }

    fn texts(flat: &[Flat]) -> Vec<Option<&str>> {
        flat.iter()
            .map(|part| match part {
                Flat::Text(text) => Some(text.as_str()),
                Flat::Tag | Flat::Interp => None,
            })
            .collect()
    }

    #[test]
    fn tag_alone_on_its_line_takes_the_line() {
        let mut flat = vec![
            text("a\n  "),
            Flat::Tag,
            text("  \n  b\n"),
            Flat::Tag,
            text("\nc"),
        ];
        strip_around_block_tags(&mut flat);
        assert_eq!(
            texts(&flat),
            [Some("a\n"), None, Some("  b\n"), None, Some("c")]
        );
    }

    #[test]
    fn inline_tag_keeps_its_line() {
        let mut flat = vec![text("a "), Flat::Tag, text(" b\n")];
        strip_around_block_tags(&mut flat);
        assert_eq!(texts(&flat), [Some("a "), None, Some(" b\n")]);
    }

    #[test]
    fn interpolation_on_the_line_means_not_alone() {
        let mut flat = vec![text("\n  "), Flat::Tag, text(" "), Flat::Interp, text("\n")];
        strip_around_block_tags(&mut flat);
        assert_eq!(
            texts(&flat),
            [Some("\n  "), None, Some(" "), None, Some("\n")]
        );
    }

    #[test]
    fn dedent_holds_interpolations_out() {
        let mut flat = vec![text("\n    a "), Flat::Interp, text("\n      b\n  ")];
        dedent(&mut flat);
        assert_eq!(texts(&flat), [Some("a "), None, Some("\n  b")]);
    }
}
