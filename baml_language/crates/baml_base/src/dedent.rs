//! Template-string dedenting.
//!
//! Shared with `sys_llm::jinja::render::preprocess_template`; both implement the
//! same algorithm and should remain in lockstep until that callsite migrates to
//! this one.
//!
//! Algorithm (BEP-049 §12, Kotlin `trimIndent` rule):
//! 1. Compute the longest common leading-whitespace *prefix* across all non-blank
//!    lines, compared character-by-character. Tabs and spaces don't mix: a
//!    tab-indented line and a space-indented line share no common prefix, so the
//!    strip column drops to zero (§12 Rule 2).
//! 2. Strip that common prefix from every line that has it; otherwise keep the
//!    line as-is (its content is whitespace-only).
//! 3. Trim leading/trailing whitespace from the overall result.
//!
//! Used by BEP-049 backtick string literals (multi-line auto-dedent, §12). The
//! legacy Jinja prompt pipeline (`sys_llm::preprocess_template`) keeps the older
//! byte-count-min variant until that path is removed (M6) — they intentionally
//! diverge on mixed tab/space indentation, which only the new backtick form specs.
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

pub fn preprocess_template(template: &str) -> String {
    let lines: Vec<&str> = template.lines().collect();

    // Longest common leading-whitespace *prefix* across non-blank lines, compared
    // char-by-char (§12 Rule 2: tabs and spaces don't mix). The strip column is
    // its byte length.
    let mut common: Option<&str> = None;
    for line in lines.iter().filter(|line| !line.trim().is_empty()) {
        let ws = &line[..leading_whitespace_bytes(line)];
        common = Some(match common {
            None => ws,
            Some(prev) => common_prefix(prev, ws),
        });
    }
    let strip = common.map_or(0, str::len);

    lines
        .iter()
        .map(|line| {
            if leading_whitespace_bytes(line) >= strip {
                strip_leading_indent(line, strip)
            } else {
                line.trim()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty() {
        assert_eq!(preprocess_template(""), "");
    }

    #[test]
    fn single_line() {
        assert_eq!(preprocess_template("hello"), "hello");
    }

    #[test]
    fn uniform_indent_stripped() {
        let input = "    hello\n    world";
        assert_eq!(preprocess_template(input), "hello\nworld");
    }

    #[test]
    fn min_indent_is_smallest() {
        // After stripping the smallest leading indent (2), the result is
        // "  hello\nworld"; the final .trim() removes the leading "  ".
        let input = "    hello\n  world";
        assert_eq!(preprocess_template(input), "hello\nworld");
    }

    #[test]
    fn blank_lines_ignored_in_min_calc() {
        let input = "    hello\n\n    world";
        assert_eq!(preprocess_template(input), "hello\n\nworld");
    }

    #[test]
    fn trims_leading_and_trailing() {
        let input = "\n    hello\n    world\n";
        assert_eq!(preprocess_template(input), "hello\nworld");
    }

    #[test]
    fn tab_and_space_indent_do_not_mix() {
        // BEP-049 §12 Rule 2: tabs and spaces don't mix — a tab-indented line and
        // a four-space-indented line share no common leading-whitespace prefix, so
        // the strip column is zero. (Byte-count-min would wrongly strip 1, eating a
        // space from the second line.) The leading tab of line 1 is removed by the
        // final `.trim()`; line 2 keeps all four spaces.
        let input = "\t- foo\n    - bar";
        assert_eq!(preprocess_template(input), "- foo\n    - bar");
    }

    #[test]
    fn nbsp_indent_does_not_panic() {
        // BEP-049 / ultrareview bug_001: when one line is indented with
        // NBSP (U+00A0, 2 UTF-8 bytes) and another with ASCII space,
        // `min_indent` computed in bytes lands inside the NBSP, and a
        // naive byte-slice `&line[1..]` panics with "byte index 1 is not
        // a char boundary". Realistic trigger: rich-text paste / macOS
        // Option+Space.
        let input = " hello\n\u{00A0}world";
        let _ = preprocess_template(input);
    }

    #[test]
    fn line_separator_indent_does_not_panic() {
        // U+2028 LINE SEPARATOR is a 3-byte Unicode whitespace char.
        // Mixing with ASCII space exposes the same byte-vs-char bug.
        let input = " xy\n\u{2028}xy";
        let _ = preprocess_template(input);
    }
}
