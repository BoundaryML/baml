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
pub fn preprocess_template(template: &str) -> String {
    let lines: Vec<&str> = template.lines().collect();

    let min_indent = lines
        .iter()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.len() - line.trim_start().len())
        .min()
        .unwrap_or(0);

    lines
        .iter()
        .map(|line| {
            if line.len() >= min_indent {
                &line[min_indent..]
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
}
