//! Line endings of formatted output, after rustfmt's `newline_style = "Auto"`.
//!
//! The database normalizes every line break in source to `\n` and keeps no
//! `\r` (see `normalize_newlines` in `baml_db`), so the printer's output has
//! only `\n` line breaks and no carriage returns. That makes converting back
//! exact: in a CRLF file every `\n` becomes `\r\n`, and nothing else changes.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NewlineStyle {
    Unix,
    Windows,
}

/// The style of the input's first line ending. Input with no `\n` gets `\n`:
/// unlike rustfmt there is no platform-native fallback, because `\n` is
/// BAML's canonical line ending. A file broken only by lone `\r`s is
/// therefore written with `\n`.
fn detect_newline_style(raw_input: &str) -> NewlineStyle {
    match raw_input.find('\n') {
        Some(pos) if raw_input[..pos].ends_with('\r') => NewlineStyle::Windows,
        _ => NewlineStyle::Unix,
    }
}

/// Converts `formatted`, which has `\n` line breaks and no `\r`, to the line
/// ending `raw_input` uses.
pub(crate) fn apply_newline_style(formatted: String, raw_input: &str) -> String {
    debug_assert!(
        !formatted.contains('\r'),
        "formatter output comes from normalized source, which has no carriage returns"
    );
    match detect_newline_style(raw_input) {
        NewlineStyle::Unix => formatted,
        NewlineStyle::Windows => formatted.replace('\n', "\r\n"),
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
    }

    #[test]
    fn applies_windows_newlines() {
        assert_eq!(
            apply_newline_style("One\nTwo\nThree\n".to_owned(), "One\r\nTwo"),
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
    fn lone_carriage_returns_are_line_breaks() {
        // `\r\r\n` is two line breaks, so formatting must not merge them,
        // whichever style the file uses.
        let source = "function f() -> string {\r\n    \"a\r\r\nb\"\r\n}\r\n";
        assert_eq!(
            format(source),
            "function f() -> string {\r\n    \"a\r\n\r\nb\"\r\n}\r\n"
        );
        let source = "function f() -> string {\n    \"a\rb\"\n}\n";
        assert_eq!(
            format(source),
            "function f() -> string {\n    \"a\nb\"\n}\n"
        );
    }

    #[test]
    fn ignored_file_is_returned_verbatim() {
        for source in [
            "// baml-format: ignore\r\nfunction  f()->int{1}\n",
            "// baml-format: ignore\nfunction  f()->int{1}\r\n\r",
            "// baml-format: ignore\r\nfunction unfinished(\n",
        ] {
            assert_eq!(format(source), source);
        }
    }
}
