//! Backtick-string dedenting.
//!
//! [`dedent_backtick`] is the whole surface: multi-line auto-dedent for BEP-049
//! backtick string literals.

/// Whitespace that represents source layout rather than authored Unicode text.
fn is_layout_whitespace(character: char) -> bool {
    matches!(character, ' ' | '\t' | '\n')
}

/// Leading whitespace, ending on a character boundary.
fn leading_whitespace(line: &str) -> &str {
    let end = line
        .char_indices()
        .find_map(|(index, character)| (!character.is_whitespace()).then_some(index))
        .unwrap_or(line.len());
    &line[..end]
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

/// Strip the layout of a backtick string literal (BEP-049 §12).
///
/// 1. Normalize `\r\n` and lone `\r` to `\n` (§AA, TypeScript parity).
/// 2. Return single-line literals unchanged.
/// 3. For multiline literals, remove every leading line break and all
///    body-final ASCII layout whitespace.
/// 4. Strip the longest exact leading-whitespace prefix shared by nonblank
///    lines, empty ASCII-layout-only lines, and discard leading empty lines.
///
/// Dedenting runs on raw literal text before escapes are decoded. An authored
/// `\n` is therefore two non-whitespace characters here and survives the trim.
pub fn dedent_backtick(text: &str) -> String {
    let normalized = normalize_newlines(text);
    if !normalized.contains('\n') {
        return normalized.into_owned();
    }
    let body = normalized
        .trim_start_matches('\n')
        .trim_end_matches(is_layout_whitespace);
    let lines: Vec<&str> = body.lines().collect();
    let strip = common_indent(&lines);
    lines
        .iter()
        .map(|line| {
            if line
                .chars()
                .any(|character| !is_layout_whitespace(character))
            {
                line.strip_prefix(strip)
                    .expect("common indent must prefix every nonblank line")
            } else {
                ""
            }
        })
        .skip_while(|line| line.is_empty())
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

/// Longest exact leading-whitespace prefix across the nonblank lines.
fn common_indent<'a>(lines: &[&'a str]) -> &'a str {
    let mut common: Option<&'a str> = None;
    for line in lines
        .iter()
        .filter(|line| !line.chars().all(is_layout_whitespace))
    {
        let ws = leading_whitespace(line);
        common = Some(match common {
            None => ws,
            Some(prev) => common_prefix(prev, ws),
        });
    }
    common.unwrap_or("")
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
    fn shared_unicode_whitespace_is_still_indentation() {
        assert_eq!(
            dedent_backtick("\n\u{00A0}hello\n\u{00A0}world\n"),
            "hello\nworld"
        );
    }

    #[test]
    fn trailing_nbsp_is_content() {
        assert_eq!(dedent_backtick("\n    hello\u{00A0}\n"), "hello\u{00A0}");
    }

    #[test]
    fn unshared_nbsp_only_line_is_content() {
        assert_eq!(
            dedent_backtick("\n    hello\n    \u{00A0}\n    world\n"),
            "hello\n\u{00A0}\nworld"
        );
    }

    #[test]
    fn backtick_single_line_preserves_boundary_whitespace() {
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
        // here can read it as layout and trim cannot eat it.
        assert_eq!(dedent_backtick(r"a\n"), r"a\n");
        assert_eq!(dedent_backtick("\n    a\\n\n"), "a\\n");
    }

    #[test]
    fn backtick_discards_blank_line_before_closer() {
        assert_eq!(dedent_backtick("\n    a\n    b\n\n    "), "a\nb");
    }

    #[test]
    fn backtick_discards_leading_and_trailing_blank_content_lines() {
        assert_eq!(dedent_backtick("\n\n    a\n\n"), "a");
    }

    #[test]
    fn backtick_keeps_trailing_spaces_inside_content() {
        // Trailing spaces on an interior line are content, not layout.
        assert_eq!(dedent_backtick("\n    a  \n    b\n"), "a  \nb");
    }

    #[test]
    fn backtick_discards_trailing_spaces_at_end_of_body() {
        assert_eq!(dedent_backtick("\n    a\n    b    \n"), "a\nb");
    }

    #[test]
    fn backtick_empties_whitespace_only_lines() {
        assert_eq!(dedent_backtick("\n    a\n     \n    b\n"), "a\n\nb");
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

    #[test]
    fn test_basic_dedent() {
        let input = r#"
            hello
            world
            "#;
        let expected = r#"hello
world"#;
        assert_eq!(dedent_backtick(input), expected);
    }

    #[test]
    fn test_mixed_indentation() {
        let input = r#"
            first line
                indented line
            back to first level
        "#;
        let expected = r#"first line
    indented line
back to first level"#;
        assert_eq!(dedent_backtick(input), expected);
    }

    #[test]
    fn test_empty_lines() {
        let input = ["", "        line1", "", "        ", "        line2"].join("\n");
        let expected = r#"line1


line2"#;
        assert_eq!(dedent_backtick(&input), expected);
    }

    #[test]
    fn test_no_indentation() {
        assert_eq!(dedent_backtick("hello\nworld"), "hello\nworld");
    }

    #[test]
    fn test_different_line_starts() {
        let input = r#"
            def function():
                # comment
                print("hello")
            "#;
        let expected = r#"def function():
    # comment
    print("hello")"#;
        assert_eq!(dedent_backtick(input), expected);
    }

    #[test]
    fn test_tabs_and_spaces() {
        let input = "\n    mixed\n\t\tindentation";
        let expected = "    mixed\n\t\tindentation";
        assert_eq!(dedent_backtick(input), expected);
    }

    #[test]
    fn test_single_line_preserves_indentation() {
        assert_eq!(dedent_backtick("    single line"), "    single line");
    }
}
