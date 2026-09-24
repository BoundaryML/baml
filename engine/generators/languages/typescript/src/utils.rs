pub fn prefix_lines(s: &str, prefix: &str) -> String {
    if s.is_empty() {
        return s.to_string();
    }
    // Escape closing JSDoc comment delimiter `*/` as `*\/` so that
    // doc comments containing `*/` cannot prematurely terminate JSDoc blocks.
    let escaped = s.replace("*/", "*\\/");
    escaped
        .lines()
        .map(|line| format!("{prefix}{line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prefix_lines_empty() {
        assert_eq!(prefix_lines("", " * "), "");
    }

    #[test]
    fn test_prefix_lines_single_line() {
        assert_eq!(prefix_lines("hello", " * "), " * hello");
    }

    #[test]
    fn test_prefix_lines_multiline() {
        assert_eq!(prefix_lines("line1\nline2", " * "), " * line1\n * line2");
    }

    #[test]
    fn test_prefix_lines_escapes_jsdoc_end_comment() {
        assert_eq!(
            prefix_lines("normal doc */ export const injected = 1; /*", " * "),
            " * normal doc *\\/ export const injected = 1; /*"
        );
    }
}
