pub use baml_db::baml_compiler_syntax::{
    ValidatedToken as Token, is_word_like, validated::tokens::*,
};
use rowan::{TextRange, TextSize};

use crate::printer::{PrintInfo, Printable, Printer, Shape};

impl Printable for BinaryOp {
    fn print(&self, _shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            BinaryOp::EqualsEquals(t) => printer.print_raw_token(t),
            BinaryOp::NotEquals(t) => printer.print_raw_token(t),
            BinaryOp::Less(t) => printer.print_raw_token(t),
            BinaryOp::Greater(t) => printer.print_raw_token(t),
            BinaryOp::LessEquals(t) => printer.print_raw_token(t),
            BinaryOp::GreaterEquals(t) => printer.print_raw_token(t),
            BinaryOp::AndAnd(t) => printer.print_raw_token(t),
            BinaryOp::OrOr(t) => printer.print_raw_token(t),
            BinaryOp::And(t) => printer.print_raw_token(t),
            BinaryOp::Pipe(t) => printer.print_raw_token(t),
            BinaryOp::Caret(t) => printer.print_raw_token(t),
            BinaryOp::Instanceof(t) => printer.print_raw_token(t),
            BinaryOp::LessLess(t) => printer.print_raw_token(t),
            BinaryOp::GreaterGreater(t) => printer.print_raw_token(t),
            BinaryOp::Plus(t) => printer.print_raw_token(t),
            BinaryOp::Minus(t) => printer.print_raw_token(t),
            BinaryOp::Star(t) => printer.print_raw_token(t),
            BinaryOp::Slash(t) => printer.print_raw_token(t),
            BinaryOp::Percent(t) => printer.print_raw_token(t),
            BinaryOp::Equals(t) => printer.print_raw_token(t),
            BinaryOp::PlusEquals(t) => printer.print_raw_token(t),
            BinaryOp::MinusEquals(t) => printer.print_raw_token(t),
            BinaryOp::StarEquals(t) => printer.print_raw_token(t),
            BinaryOp::SlashEquals(t) => printer.print_raw_token(t),
            BinaryOp::PercentEquals(t) => printer.print_raw_token(t),
            BinaryOp::AndEquals(t) => printer.print_raw_token(t),
            BinaryOp::PipeEquals(t) => printer.print_raw_token(t),
            BinaryOp::CaretEquals(t) => printer.print_raw_token(t),
            BinaryOp::LessLessEquals(t) => printer.print_raw_token(t),
            BinaryOp::GreaterGreaterEquals(t) => printer.print_raw_token(t),
            BinaryOp::QuestionQuestion(t) => printer.print_raw_token(t),
        }
        PrintInfo::default_single_line()
    }
    fn leftmost_token(&self) -> TextRange {
        self.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.span()
    }
}

impl Printable for UnaryOp {
    fn print(&self, _shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            UnaryOp::Not(t) => printer.print_raw_token(t),
            UnaryOp::Minus(t) => printer.print_raw_token(t),
        }
        PrintInfo::default_single_line()
    }
    fn leftmost_token(&self) -> TextRange {
        self.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.span()
    }
}

impl Printable for QuotedString {
    fn print(&self, _shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(self);
        PrintInfo {
            multi_lined: printer.input[self.span()].contains('\n'),
        }
    }
    fn leftmost_token(&self) -> TextRange {
        TextRange::new(
            self.token_span.start(),
            self.token_span.start() + TextSize::from(1),
        )
    }
    fn rightmost_token(&self) -> TextRange {
        TextRange::new(
            self.token_span.end() - TextSize::from(1),
            self.token_span.end(),
        )
    }
}

impl Printable for RawString {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let text = &printer.input[self.span()];
        let multi_lined = text.contains('\n');
        if !multi_lined {
            // print as-is
            printer.print_raw_token(self);
            return PrintInfo { multi_lined };
        }

        // we need to re-organize the interior
        let (Some(start_quote), Some(end_quote)) = (text.find('"'), text.rfind('"')) else {
            // should never happen, but print as-is if it does
            printer.print_raw_token(self);
            return PrintInfo { multi_lined };
        };
        if end_quote <= start_quote {
            // should never happen, but print as-is if it does
            printer.print_raw_token(self);
            return PrintInfo { multi_lined };
        }

        let interior = &text[start_quote + 1..end_quote].trim();
        let mut lines = interior.lines();
        let Some(first_line) = lines.next() else {
            // Interior is empty after trim (e.g. `#"\n"#`) — print as-is.
            printer.print_raw_token(self);
            return PrintInfo { multi_lined };
        };
        let min_indent = lines
            .clone()
            .map(|line| {
                let count = line.bytes().take_while(|c| *c == b' ').count();
                if count == line.len() {
                    // it is all spaces
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
                // This line must be all spaces since otherwise it would have affected `min_indent`.
                // So we can print an empty line.
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
    fn leftmost_token(&self) -> TextRange {
        TextRange::new(
            self.token_span.start(),
            self.token_span.start() + TextSize::from(1),
        )
    }
    fn rightmost_token(&self) -> TextRange {
        TextRange::new(
            self.token_span.end() - TextSize::from(1),
            self.token_span.end(),
        )
    }
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

impl Printable for BacktickString {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let text = &printer.input[self.span()];
        let multi_lined = text.contains('\n');
        let reindented = if self.dedent_safe {
            reindent_backtick(text, shape.indent, printer.config.indent_width)
        } else {
            None
        };
        match reindented {
            Some(reindented) => printer.print_str(&reindented),
            None => printer.print_raw_token(self),
        }
        PrintInfo { multi_lined }
    }
    fn leftmost_token(&self) -> TextRange {
        TextRange::new(
            self.token_span.start(),
            self.token_span.start() + TextSize::from(1),
        )
    }
    fn rightmost_token(&self) -> TextRange {
        TextRange::new(
            self.token_span.end() - TextSize::from(1),
            self.token_span.end(),
        )
    }
}

impl Printable for ByteString {
    fn print(&self, _shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(self);
        PrintInfo { multi_lined: false }
    }
    fn leftmost_token(&self) -> TextRange {
        TextRange::new(
            self.token_span.start(),
            self.token_span.start() + TextSize::from(1),
        )
    }
    fn rightmost_token(&self) -> TextRange {
        TextRange::new(
            self.token_span.end() - TextSize::from(1),
            self.token_span.end(),
        )
    }
}
