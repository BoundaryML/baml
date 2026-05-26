//! String-literal escape decoding.
//!
//! Single source of truth for escape sequences across the BAML compiler. Used
//! by both regular `"..."` literals and BEP-049 backtick `` `...` `` literals.
//!
//! Recognized escapes (always): `\n`, `\t`, `\r`, `\0`, `\\`, `\"`.
//!
//! Backtick-literal-only escapes: `` \` `` and `\$` (covers the `\${`
//! disambiguation from §8 of BEP-049 — backslash before `$` always produces a
//! literal `$`, so `\${name}` renders as the text `${name}`).
//!
//! Unknown escapes preserve the backslash followed by the next character.

/// Decode escapes for a regular `"..."` string literal body (i.e., the text
/// between the surrounding quotes, with quotes already stripped).
pub fn unescape_string_literal(input: &str) -> String {
    unescape_with(input, EscapeFlavor::Quote)
}

/// Decode escapes for a BEP-049 backtick string literal body (i.e., the text
/// between the surrounding backtick runs, with delimiters already stripped).
pub fn unescape_backtick_string_literal(input: &str) -> String {
    unescape_with(input, EscapeFlavor::Backtick)
}

#[derive(Copy, Clone)]
enum EscapeFlavor {
    Quote,
    Backtick,
}

fn unescape_with(input: &str, flavor: EscapeFlavor) -> String {
    let mut result = String::with_capacity(input.len());
    let mut chars = input.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            result.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => result.push('\n'),
            Some('t') => result.push('\t'),
            Some('r') => result.push('\r'),
            Some('0') => result.push('\0'),
            Some('\\') => result.push('\\'),
            Some('"') => result.push('"'),
            Some('`') if matches!(flavor, EscapeFlavor::Backtick) => result.push('`'),
            Some('$') if matches!(flavor, EscapeFlavor::Backtick) => result.push('$'),
            Some(other) => {
                result.push('\\');
                result.push(other);
            }
            None => result.push('\\'),
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quote_decodes_supported_escapes() {
        assert_eq!(unescape_string_literal(r"line\nbreak"), "line\nbreak");
        assert_eq!(unescape_string_literal(r"tab\there"), "tab\there");
        assert_eq!(unescape_string_literal(r"cr\rhere"), "cr\rhere");
        assert_eq!(unescape_string_literal(r"nul\0here"), "nul\0here");
        assert_eq!(unescape_string_literal(r"back\\slash"), "back\\slash");
        assert_eq!(unescape_string_literal(r#"a\"b"#), "a\"b");
    }

    #[test]
    fn quote_preserves_unknown_sequences() {
        assert_eq!(unescape_string_literal(r"\x41"), "\\x41");
        assert_eq!(unescape_string_literal(r"\u0041"), "\\u0041");
    }

    #[test]
    fn quote_preserves_trailing_backslash() {
        assert_eq!(unescape_string_literal("trailing\\"), "trailing\\");
    }

    #[test]
    fn quote_handles_empty_and_plain_text() {
        assert_eq!(unescape_string_literal(""), "");
        assert_eq!(unescape_string_literal("plain text"), "plain text");
    }

    #[test]
    fn quote_does_not_decode_backtick_or_dollar() {
        // Regular strings keep \` and \$ as literal backslash + char.
        assert_eq!(unescape_string_literal(r"a\`b"), "a\\`b");
        assert_eq!(unescape_string_literal(r"a\$b"), "a\\$b");
    }

    #[test]
    fn backtick_decodes_standard_plus_backtick_and_dollar() {
        assert_eq!(
            unescape_backtick_string_literal(r"line\nbreak"),
            "line\nbreak"
        );
        assert_eq!(unescape_backtick_string_literal(r"a\`b"), "a`b");
        assert_eq!(unescape_backtick_string_literal(r"a\${x}b"), "a${x}b");
    }

    #[test]
    fn backtick_preserves_unknown_sequences() {
        assert_eq!(unescape_backtick_string_literal(r"\x41"), "\\x41");
    }
}
