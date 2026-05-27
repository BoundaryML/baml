//! String-literal escape decoding.
//!
//! Single source of truth for escape sequences across the BAML compiler. Used
//! by both regular `"..."` literals and BEP-049 backtick `` `...` `` literals.
//!
//! Recognized escapes (always): `\n`, `\t`, `\r`, `\0`, `\b`, `\v`, `\f`,
//! `\\`, `\"`. The C-style control trio (`\b`, `\v`, `\f`) is included for
//! TypeScript parity (BEP-049 §BB); see `typescript-go` scanner.go:1721-1736.
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
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        // BEP-049 §AA (TS parity): normalize line endings in backtick
        // literal text. `\r\n` → `\n`, lone `\r` → `\n`. Mirrors
        // typescript-go's scanner (scanner.go:1650-1660).
        if matches!(flavor, EscapeFlavor::Backtick) && c == '\r' {
            // Consume an immediately-following `\n` (the CRLF case).
            if chars.peek() == Some(&'\n') {
                chars.next();
            }
            result.push('\n');
            continue;
        }
        if c != '\\' {
            result.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => result.push('\n'),
            Some('t') => result.push('\t'),
            Some('r') => result.push('\r'),
            Some('0') => result.push('\0'),
            // BEP-049 §BB (TS parity): extended C-style escapes.
            // ASCII control characters: BS (0x08), VT (0x0B), FF (0x0C).
            Some('b') => result.push('\u{0008}'),
            Some('v') => result.push('\u{000B}'),
            Some('f') => result.push('\u{000C}'),
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

    #[test]
    fn backtick_extended_escapes_b_v_f() {
        // BEP-049 §BB / TypeScript-go scanner.go:1721-1736
        assert_eq!(unescape_backtick_string_literal(r"\b"), "\u{0008}");
        assert_eq!(unescape_backtick_string_literal(r"\v"), "\u{000B}");
        assert_eq!(unescape_backtick_string_literal(r"\f"), "\u{000C}");
    }

    #[test]
    fn backtick_normalizes_crlf_to_lf() {
        // BEP-049 §AA / TypeScript-go scanner.go:1650-1660: `\r\n` becomes `\n`.
        assert_eq!(
            unescape_backtick_string_literal("line1\r\nline2"),
            "line1\nline2"
        );
    }

    #[test]
    fn backtick_normalizes_lone_cr_to_lf() {
        // Bare CR (old Mac line endings) — also normalized.
        assert_eq!(
            unescape_backtick_string_literal("line1\rline2"),
            "line1\nline2"
        );
    }

    #[test]
    fn backtick_normalizes_mixed_line_endings() {
        // CRLF, CR, and LF in sequence all yield single LFs.
        assert_eq!(
            unescape_backtick_string_literal("a\r\nb\rc\nd"),
            "a\nb\nc\nd"
        );
    }

    #[test]
    fn quote_flavor_does_not_normalize_cr() {
        // The CR/CRLF normalization is backtick-specific (BEP-049 §12).
        // Regular `"..."` literals keep CR as-is.
        assert_eq!(unescape_string_literal("a\r\nb"), "a\r\nb");
    }

    #[test]
    fn quote_flavor_also_gets_extended_escapes() {
        // \b, \v, \f are universally valid C-style escapes — apply to both
        // flavors so the canonical helper is consistent.
        assert_eq!(unescape_string_literal(r"\b"), "\u{0008}");
        assert_eq!(unescape_string_literal(r"\v"), "\u{000B}");
        assert_eq!(unescape_string_literal(r"\f"), "\u{000C}");
    }
}
