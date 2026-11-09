use baml_syntax::{
    nodes::{
        AnyDoc, AnyTemplatePart, BacktickString, Directive, DocLine, FileTree, HeaderLine,
        LiteralExpr, Name, StringLit,
    },
    ops::{BinaryOp, BinaryOpTok, OpTok, Operator},
    token::TokenKind,
    tree::{Id, TokenRange},
};

use super::union_printable;
use crate::printer::{PrintInfo, Printable, Printer, Shape, Spanned};

/// A name token prints as written.
impl Printable for Id<Name> {
    fn print(self, _shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_token(printer.tree.get(self).token);
        PrintInfo::default_single_line()
    }
}

impl Printable for Id<LiteralExpr> {
    fn print(self, _shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_token(printer.tree.get(self).token);
        PrintInfo::default_single_line()
    }
}

impl Printable for Id<DocLine> {
    fn print(self, _shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_token(printer.tree.get(self).token);
        PrintInfo::default_single_line()
    }
}

impl Printable for Id<Directive> {
    fn print(self, _shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_token(printer.tree.get(self).token);
        PrintInfo::default_single_line()
    }
}

impl Printable for Id<HeaderLine> {
    fn print(self, _shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_token(printer.tree.get(self).token);
        PrintInfo::default_single_line()
    }
}

union_printable!(AnyDoc {
    Doc,
    Directive,
    Header
});

/// An operator's tokens are adjacent, so they print as one run.
impl<O: Operator> Spanned for OpTok<O> {
    fn tokens(self, _tree: &FileTree) -> TokenRange {
        self.range()
    }
}

impl<O: Operator> Printable for OpTok<O> {
    fn print(self, _shape: Shape, printer: &mut Printer) -> PrintInfo {
        for token in OpTok::tokens(self) {
            let range = printer.tree.tokens().range(token);
            printer.print_input_range(range);
        }
        PrintInfo::default_single_line()
    }
}

pub(crate) trait BinaryOpExt {
    /// Whether this operator also has a prefix form (`-b`) that can start a
    /// new statement. The formatter conservatively groups wrapped chains
    /// containing it, although its spaced `- b` layout is a continuation.
    fn begins_statement_at_line_start(self) -> bool;
}
impl BinaryOpExt for BinaryOpTok {
    fn begins_statement_at_line_start(self) -> bool {
        self.op() == BinaryOp::Sub
    }
}

/// A string literal: `"…"`, `r"…"`, `b"…"`, or a multi-line `#"…"#`, which is
/// re-indented.
impl Printable for Id<StringLit> {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let tree = printer.tree;
        let token = tree.get(self).token;
        let text = tree.token_text(token.idx);
        match tree.tokens().kind(token.idx) {
            TokenKind::LegacyHashString | TokenKind::UnclosedHashString => {
                print_hash_string(text, &shape, printer)
            }
            TokenKind::ByteString | TokenKind::UnclosedByteString => {
                printer.print_token(token);
                PrintInfo { multi_lined: false }
            }
            _ => {
                printer.print_token(token);
                PrintInfo {
                    multi_lined: text.contains('\n'),
                }
            }
        }
    }
}

/// A multi-line `#"…"#` is re-indented: its interior's common indentation
/// is replaced by one level below the string's own line.
fn print_hash_string(text: &str, shape: &Shape, printer: &mut Printer) -> PrintInfo {
    let multi_lined = text.contains('\n');
    if !multi_lined {
        printer.print_str(text);
        return PrintInfo { multi_lined };
    }
    let (Some(start_quote), Some(end_quote)) = (text.find('"'), text.rfind('"')) else {
        printer.print_str(text);
        return PrintInfo { multi_lined };
    };
    if end_quote <= start_quote {
        printer.print_str(text);
        return PrintInfo { multi_lined };
    }
    let interior = &text[start_quote + 1..end_quote].trim();
    let mut lines = interior.lines();
    let Some(first_line) = lines.next() else {
        printer.print_str(text);
        return PrintInfo { multi_lined };
    };
    let min_indent = lines
        .clone()
        .map(|line| {
            let count = line.bytes().take_while(|c| *c == b' ').count();
            if count == line.len() {
                usize::MAX
            } else {
                count
            }
        })
        .min()
        .unwrap_or(0);
    let inner_base_indent = shape.indent + printer.config.indent_width;
    printer.print_str(&text[..=start_quote]);
    printer.print_newline();
    printer.print_spaces(inner_base_indent);
    printer.print_str(first_line.trim_start_matches(' '));
    for line in lines {
        if line.len() <= min_indent {
            printer.print_newline();
            continue;
        }
        let (removed_indent, line) = line.split_at(min_indent);
        debug_assert!(
            removed_indent.bytes().all(|c| c == b' '),
            "should not have removed non-indent"
        );
        debug_assert!(!line.is_empty(), "should have been handled above");
        printer.print_newline();
        printer.print_spaces(inner_base_indent);
        printer.print_str(line);
    }
    printer.print_newline();
    printer.print_spaces(shape.indent);
    printer.print_str(&text[end_quote..]);
    PrintInfo { multi_lined }
}

/// A backtick string prints as written, re-indented when that cannot change
/// its value.
impl Printable for Id<BacktickString> {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let text = printer.text_of(self);
        let multi_lined = text.contains('\n');
        let reindented = if dedent_safe(printer, self) {
            reindent_backtick(text, shape.indent, printer.config.indent_width)
        } else {
            None
        };
        match reindented {
            Some(reindented) => printer.print_str(&reindented),
            None => printer.print_str(text),
        }
        PrintInfo { multi_lined }
    }
}

/// Whether re-indenting the literal cannot change its value: no block tag,
/// and no interpolation spanning lines.
fn dedent_safe(printer: &Printer, id: Id<BacktickString>) -> bool {
    let tree = printer.tree;
    !tree
        .list(tree.get(id).parts)
        .iter()
        .any(|part| match *part {
            AnyTemplatePart::For(_) | AnyTemplatePart::If(_) => true,
            AnyTemplatePart::Interp(interp) => printer.text_of(interp).contains('\n'),
            AnyTemplatePart::Chunk(_) => false,
            AnyTemplatePart::Bogus(_) => unreachable!("the formatter prints only error-free trees"),
        })
}

/// Re-indent a multi-line backtick literal `text` so its interior sits one level
/// past `indent`, or return `None` to print it verbatim (single line, malformed,
/// or the re-indent would change the runtime value).
fn reindent_backtick(text: &str, indent: usize, indent_width: usize) -> Option<String> {
    if !text.contains('\n') {
        return None;
    }
    // The delimiter is a run of N backticks on each side (tick ladder).
    let ticks = text.bytes().take_while(|&c| c == b'`').count();
    if ticks == 0 || text.len() < ticks * 2 {
        return None;
    }
    let inner = &text[ticks..text.len() - ticks];

    // Source-level dedent: strip the common leading-whitespace prefix and the
    // delimiters' own line breaks, exactly as the compiler's §12 dedent does,
    // and on the raw source for the same reason it does — so escapes and
    // `${...}` stay intact and the printed form remains valid source.
    let dedented = baml_db::dedent::dedent_backtick(inner);
    if dedented.is_empty() {
        return None;
    }

    let base = indent + indent_width;
    let mut candidate_inner = String::from("\n");
    for (i, line) in dedented.lines().enumerate() {
        if i > 0 {
            candidate_inner.push('\n');
        }
        if !line.is_empty() {
            candidate_inner.extend(std::iter::repeat_n(' ', base));
            candidate_inner.push_str(line);
        }
    }
    candidate_inner.push('\n');
    candidate_inner.extend(std::iter::repeat_n(' ', indent));

    // Bail to verbatim unless the runtime value (§12 dedented, then escapes
    // decoded — the compiler's order) is byte-identical for the original and
    // re-indented interiors.
    let value = |s: &str| {
        baml_db::escape::unescape_backtick_string_literal(&baml_db::dedent::dedent_backtick(s))
    };
    if value(inner) != value(&candidate_inner) {
        return None;
    }

    Some(format!(
        "{}{}{}",
        &text[..ticks],
        candidate_inner,
        &text[text.len() - ticks..]
    ))
}
