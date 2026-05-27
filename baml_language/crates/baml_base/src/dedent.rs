//! Template-string dedenting.
//!
//! Shared with `sys_llm::jinja::render::preprocess_template`; both implement the
//! same algorithm and should remain in lockstep until that callsite migrates to
//! this one.
//!
//! Algorithm:
//! 1. Compute the minimum leading-whitespace count across all non-blank lines.
//! 2. Strip that many leading bytes from every line that has them; otherwise
//!    keep the line as-is (its content is whitespace-only).
//! 3. Trim leading/trailing whitespace from the overall result.
//!
//! Used by:
//! - BEP-049 backtick string literals (multi-line auto-dedent, §12).
//! - The legacy Jinja prompt pipeline (`sys_llm::preprocess_template`).
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

pub fn preprocess_template(template: &str) -> String {
    let lines: Vec<&str> = template.lines().collect();

    let min_indent = lines
        .iter()
        .filter(|line| !line.trim().is_empty())
        .map(|line| leading_whitespace_bytes(line))
        .min()
        .unwrap_or(0);

    lines
        .iter()
        .map(|line| {
            if leading_whitespace_bytes(line) >= min_indent {
                strip_leading_indent(line, min_indent)
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
