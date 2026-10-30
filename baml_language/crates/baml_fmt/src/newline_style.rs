//! Line endings of formatted output, after rustfmt's `newline_style = "Auto"`.
//!
//! The compiler sees source with `\r\n` normalized to `\n`, so the printer
//! only emits `\n`. The output is converted back to the line ending the input
//! used, so formatting a CRLF file (a Windows checkout) changes only what the
//! formatter means to change.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NewlineStyle {
    Unix,
    Windows,
}

/// The style of the input's first line ending. Input with no line ending
/// gets `\n`: unlike rustfmt there is no platform-native fallback, because
/// `\n` is BAML's canonical line ending.
fn detect_newline_style(raw_input: &str) -> NewlineStyle {
    match raw_input.find('\n') {
        Some(pos) if raw_input[..pos].ends_with('\r') => NewlineStyle::Windows,
        _ => NewlineStyle::Unix,
    }
}

/// Converts every line ending in `formatted` to the style `raw_input` uses.
/// A lone `\r` is kept.
pub fn apply_newline_style(formatted: String, raw_input: &str) -> String {
    match detect_newline_style(raw_input) {
        NewlineStyle::Unix => {
            if formatted.contains("\r\n") {
                formatted.replace("\r\n", "\n")
            } else {
                formatted
            }
        }
        NewlineStyle::Windows => {
            let mut out = String::with_capacity(formatted.len() + formatted.len() / 16);
            let mut chars = formatted.chars().peekable();
            while let Some(c) = chars.next() {
                match c {
                    '\n' => out.push_str("\r\n"),
                    '\r' if chars.peek() == Some(&'\n') => {}
                    c => out.push(c),
                }
            }
            out
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_unix_newlines() {
        assert_eq!(detect_newline_style("One\nTwo\nThree"), NewlineStyle::Unix);
    }

    #[test]
    fn detects_windows_newlines() {
        assert_eq!(
            detect_newline_style("One\r\nTwo\r\nThree"),
            NewlineStyle::Windows
        );
    }

    #[test]
    fn detects_windows_newlines_with_multibyte_char_on_first_line() {
        assert_eq!(
            detect_newline_style("A \u{1F3A2} of a first line\r\nTwo\r\nThree"),
            NewlineStyle::Windows
        );
    }

    #[test]
    fn first_line_ending_decides() {
        assert_eq!(detect_newline_style("One\nTwo\r\n"), NewlineStyle::Unix);
        assert_eq!(detect_newline_style("One\r\nTwo\n"), NewlineStyle::Windows);
    }

    #[test]
    fn no_newline_is_unix() {
        assert_eq!(detect_newline_style("One Two Three"), NewlineStyle::Unix);
        assert_eq!(detect_newline_style(""), NewlineStyle::Unix);
    }

    #[test]
    fn lone_carriage_return_on_first_line_is_not_windows() {
        assert_eq!(detect_newline_style("One\rTwo\nThree"), NewlineStyle::Unix);
    }

    #[test]
    fn applies_unix_newlines() {
        assert_eq!(
            apply_newline_style("One\nTwo\nThree".to_owned(), "One\nTwo"),
            "One\nTwo\nThree"
        );
        assert_eq!(
            apply_newline_style("One\r\nTwo\nThree".to_owned(), "One\nTwo"),
            "One\nTwo\nThree"
        );
    }

    #[test]
    fn applies_windows_newlines() {
        assert_eq!(
            apply_newline_style("One\nTwo\nThree".to_owned(), "One\r\nTwo"),
            "One\r\nTwo\r\nThree"
        );
        assert_eq!(
            apply_newline_style("One\nTwo\r\nThree\n".to_owned(), "One\r\nTwo"),
            "One\r\nTwo\r\nThree\r\n"
        );
    }

    fn format(source: &str) -> String {
        crate::format(source, &crate::FormatOptions::default()).expect("source formats")
    }

    #[test]
    fn formatted_crlf_file_is_unchanged() {
        let lf = "function f() -> string {\n    \"a\nb\"\n}\n";
        assert_eq!(format(lf), lf);
        let crlf = lf.replace('\n', "\r\n");
        assert_eq!(format(&crlf), crlf);
    }

    #[test]
    fn unformatted_crlf_file_is_formatted_with_crlf() {
        let lf = "function f()->string{\n\"a\nb\"\n}\n";
        let crlf = lf.replace('\n', "\r\n");
        assert_eq!(format(&crlf), format(lf).replace('\n', "\r\n"));
    }

    #[test]
    fn mixed_line_endings_follow_the_first_line() {
        let source = "function f() -> int {\r\n    1\n}\n";
        assert_eq!(format(source), "function f() -> int {\r\n    1\r\n}\r\n");
    }

    #[test]
    fn keeps_lone_carriage_returns() {
        assert_eq!(
            apply_newline_style("One\nTwo\rDrei".to_owned(), "One\r\n"),
            "One\r\nTwo\rDrei"
        );
        assert_eq!(
            apply_newline_style("One\nTwo\rDrei".to_owned(), "One\n"),
            "One\nTwo\rDrei"
        );
    }
}
