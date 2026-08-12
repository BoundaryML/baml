//! Backtick-string dedenting.
//!
//! [`dedent_backtick`] is the whole surface: multi-line auto-dedent for BEP-049
//! backtick string literals. It removes the layout the author used to lay a
//! literal out across several lines, and nothing else — see its docs for why
//! that distinction has to be exact.
// Walk leading whitespace by *char* so we never split a multi-byte
// Unicode whitespace codepoint (NBSP U+00A0 = 2 bytes, LINE SEPARATOR
// U+2028 = 3 bytes, etc.). A naive `line.len() - line.trim_start().len()`
// mixed with `&line[min_indent..]` panics when an NBSP-indented line is
// sliced at a byte offset derived from a sibling ASCII-indented line.
fn leading_whitespace_bytes(line: &str) -> usize {
    line.chars()
        .take_while(|c| c.is_whitespace())
        .map(char::len_utf8)
        .sum()
}

// Strip leading-whitespace chars from a line until we've stripped at
// least `target_bytes` bytes — always stopping on a char boundary, even
// if that means over-stripping by a few bytes when a multi-byte
// whitespace char straddles the target. (Under-stripping into the
// middle of a char would panic; over-stripping at most one whitespace
// char is benign — the line was leading-whitespace anyway.)
fn strip_leading_indent(line: &str, target_bytes: usize) -> &str {
    if target_bytes == 0 {
        return line;
    }
    let mut consumed = 0usize;
    let mut split_at = 0usize;
    for c in line.chars() {
        if !c.is_whitespace() {
            break;
        }
        consumed += c.len_utf8();
        split_at += c.len_utf8();
        if consumed >= target_bytes {
            break;
        }
    }
    &line[split_at..]
}

/// Longest common prefix of two strings, ending on a char boundary.
fn common_prefix<'a>(a: &'a str, b: &str) -> &'a str {
    let end = a
        .char_indices()
        .zip(b.chars())
        .take_while(|((_, ca), cb)| ca == cb)
        .map(|((i, ca), _)| i + ca.len_utf8())
        .last()
        .unwrap_or(0);
    &a[..end]
}

/// Strip the layout of a multi-line backtick string literal (BEP-049 §12),
/// leaving everything the author actually wrote.
///
/// Three steps, in order:
///
/// 1. Normalize `\r\n` and lone `\r` to `\n` (§AA, TypeScript parity).
/// 2. Drop the line break that follows the opening delimiter and the line break
///    plus indentation that precedes the closing one. These belong to the
///    delimiters, not the content.
/// 3. Strip the longest common leading-whitespace prefix from the remaining
///    lines (§12 Rule 2: compared char-by-char, so tabs and spaces don't mix).
///
/// What it deliberately does *not* do is trim. Blank lines the author left at
/// either end, trailing spaces, and a `\n` written as an escape all survive.
/// That last one is why a blanket trim was wrong: it silently ate the newline
/// in `` `${host}\n` ``, so a generated `/etc/hostname` came out without one.
///
/// Runs on the *raw* literal text, before escapes are decoded, so an authored
/// `\n` is still two opaque characters here and can never be mistaken for a
/// line break of the layout. A single-line literal is returned unchanged: with
/// no layout to strip, its leading and trailing spaces are content.
pub fn dedent_backtick(text: &str) -> String {
    if !text.contains(['\n', '\r']) {
        return text.to_string();
    }
    let normalized = normalize_newlines(text);
    let mut body = normalized.as_ref();

    // The line break after the opening delimiter (any spaces/tabs in front of
    // it are part of that same break).
    if let Some(rest) = body.trim_start_matches([' ', '\t']).strip_prefix('\n') {
        body = rest;
    }
    // The line break before the closing delimiter, plus the indent that lines
    // the delimiter up with the code around it.
    if let Some(rest) = body.trim_end_matches([' ', '\t']).strip_suffix('\n') {
        body = rest;
    }

    // Split on '\n' rather than `str::lines`, which would silently swallow a
    // trailing newline the author asked for (a blank line before the closer).
    let lines: Vec<&str> = body.split('\n').collect();
    let strip = common_indent(&lines);
    lines
        .iter()
        .map(|line| {
            if leading_whitespace_bytes(line) >= strip {
                strip_leading_indent(line, strip)
            } else {
                // Whitespace-only line shorter than the common indent.
                ""
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// `\r\n` and lone `\r` become `\n` (§AA, mirroring typescript-go's scanner).
/// Borrows when there is nothing to change.
fn normalize_newlines(text: &str) -> std::borrow::Cow<'_, str> {
    if !text.contains('\r') {
        return std::borrow::Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\r' {
            if chars.peek() == Some(&'\n') {
                chars.next();
            }
            out.push('\n');
        } else {
            out.push(c);
        }
    }
    std::borrow::Cow::Owned(out)
}

/// Byte length of the longest common leading-whitespace prefix across the
/// non-blank lines.
fn common_indent(lines: &[&str]) -> usize {
    let mut common: Option<&str> = None;
    for line in lines.iter().filter(|line| !line.trim().is_empty()) {
        let ws = &line[..leading_whitespace_bytes(line)];
        common = Some(match common {
            None => ws,
            Some(prev) => common_prefix(prev, ws),
        });
    }
    common.map_or(0, str::len)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty() {
        assert_eq!(dedent_backtick(""), "");
    }

    #[test]
    fn single_line() {
        assert_eq!(dedent_backtick("hello"), "hello");
    }

    #[test]
    fn uniform_indent_stripped() {
        assert_eq!(dedent_backtick("    hello\n    world"), "hello\nworld");
    }

    #[test]
    fn min_indent_is_smallest() {
        // The common prefix is the *shortest* of the two indents, so line one
        // keeps the two spaces that line two doesn't have.
        assert_eq!(dedent_backtick("    hello\n  world"), "  hello\nworld");
    }

    #[test]
    fn blank_lines_ignored_in_min_calc() {
        assert_eq!(dedent_backtick("    hello\n\n    world"), "hello\n\nworld");
    }

    #[test]
    fn tab_and_space_indent_do_not_mix() {
        // BEP-049 §12 Rule 2: a tab-indented line and a four-space-indented
        // line share no common leading-whitespace prefix, so the strip column
        // is zero. (A byte-count min would wrongly strip 1, eating a space from
        // the second line.) Both lines keep their own indent.
        assert_eq!(dedent_backtick("\t- foo\n    - bar"), "\t- foo\n    - bar");
    }

    #[test]
    fn nbsp_indent_is_preserved() {
        // BEP-049 / ultrareview bug_001: when one line is indented with
        // NBSP (U+00A0, 2 UTF-8 bytes) and another with ASCII space,
        // a strip column computed in bytes lands inside the NBSP, and a
        // naive byte-slice `&line[1..]` panics with "byte index 1 is not
        // a char boundary". Realistic trigger: rich-text paste / macOS
        // Option+Space.
        //
        // Not panicking is the floor. NBSP and space are different characters,
        // so by Rule 2 they share no prefix and the strip column is zero:
        // both indents are the author's and come through byte-for-byte.
        assert_eq!(
            dedent_backtick(" hello\n\u{00A0}world"),
            " hello\n\u{00A0}world"
        );
    }

    #[test]
    fn line_separator_indent_is_preserved() {
        // U+2028 LINE SEPARATOR is a 3-byte Unicode whitespace char.
        // Mixing with ASCII space exposes the same byte-vs-char bug, and the
        // same Rule 2 outcome: nothing common, so nothing stripped.
        assert_eq!(dedent_backtick(" xy\n\u{2028}xy"), " xy\n\u{2028}xy");
    }

    #[test]
    fn backtick_single_line_is_untouched() {
        // No layout to strip, so the spaces are content.
        assert_eq!(dedent_backtick("  hello  "), "  hello  ");
    }

    #[test]
    fn backtick_strips_indent_and_delimiter_line_breaks() {
        let input = "\n        line one\n        line two\n    ";
        assert_eq!(dedent_backtick(input), "line one\nline two");
    }

    #[test]
    fn backtick_keeps_relative_indent() {
        let input = "\n    header\n        bullet\n    footer\n";
        assert_eq!(dedent_backtick(input), "header\n    bullet\nfooter");
    }

    #[test]
    fn backtick_keeps_trailing_escaped_newline() {
        // B-1474. The escape is two raw characters at this point, so nothing
        // here can read it as layout, and no trim runs to eat it either.
        assert_eq!(dedent_backtick(r"a\n"), r"a\n");
        assert_eq!(dedent_backtick("\n    a\\n\n"), "a\\n");
    }

    #[test]
    fn backtick_blank_line_before_closer_yields_trailing_newline() {
        // Only *one* line break belongs to the closing delimiter; the blank
        // line the author left in front of it is content.
        assert_eq!(dedent_backtick("\n    a\n    b\n\n    "), "a\nb\n");
    }

    #[test]
    fn backtick_keeps_leading_and_trailing_blank_content_lines() {
        assert_eq!(dedent_backtick("\n\n    a\n\n"), "\na\n");
    }

    #[test]
    fn backtick_keeps_trailing_spaces_inside_content() {
        // Trailing spaces on an interior line are content, not layout.
        assert_eq!(dedent_backtick("\n    a  \n    b\n"), "a  \nb");
    }

    #[test]
    fn backtick_escape_does_not_count_as_indentation() {
        // `\t` is a backslash and a `t`, not whitespace: it must not join the
        // common prefix, and must not be stripped off the front of a line.
        assert_eq!(dedent_backtick("\n  a\n  \\tb\n"), "a\n\\tb");
    }

    #[test]
    fn backtick_normalizes_crlf() {
        assert_eq!(dedent_backtick("\r\n    a\r\n    b\r\n"), "a\nb");
    }
}
