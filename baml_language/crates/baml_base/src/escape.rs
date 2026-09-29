//! String-literal escape decoding.
//!
//! Single source of truth for escape sequences across the BAML compiler. Used
//! by both regular `"..."` literals and BEP-049 backtick `` `...` `` literals.
//!
//! Recognized escapes (always): `\n`, `\t`, `\r`, `\0`, `\b`, `\v`, `\f`,
//! `\\`, `\"`, `\'`. The C-style control trio (`\b`, `\v`, `\f`) is included
//! for TypeScript parity (BEP-049 §BB); see `typescript-go` scanner.go:1721-1736.
//!
//! Code-point escapes (JavaScript semantics):
//! - `\xHH` — exactly two hex digits, the code point U+0000..=U+00FF.
//! - `\uHHHH` — exactly four hex digits. A high surrogate immediately
//!   followed by a `\uHHHH` low surrogate combines into one code point
//!   (`😀` is U+1F600); an unpaired surrogate is invalid.
//! - `\u{H...}` — one to six hex digits, any Unicode scalar value.
//!
//! Backtick-literal-only escapes: `` \` `` and `\$` (covers the `\${`
//! disambiguation from §8 of BEP-049 — backslash before `$` always produces a
//! literal `$`, so `\${name}` renders as the text `${name}`).
//!
//! Unknown and malformed escapes preserve the backslash and the text that
//! follows it. The compiler reports them through [`string_literal_escape_issues`]
//! / [`backtick_string_literal_escape_issues`].

/// Decode escapes for a regular `"..."` string literal body (i.e., the text
/// between the surrounding quotes, with quotes already stripped).
pub fn unescape_string_literal(input: &str) -> String {
    unescape_with(input, EscapeFlavor::Quote, &mut Vec::new())
}

/// Decode escapes for a BEP-049 backtick string literal body (i.e., the text
/// between the surrounding backtick runs, with delimiters already stripped).
pub fn unescape_backtick_string_literal(input: &str) -> String {
    unescape_with(input, EscapeFlavor::Backtick, &mut Vec::new())
}

/// Unknown or malformed escape sequences in a regular `"..."` string literal
/// body. Offsets are byte offsets into `input`.
pub fn string_literal_escape_issues(input: &str) -> Vec<EscapeIssue> {
    let mut issues = Vec::new();
    unescape_with(input, EscapeFlavor::Quote, &mut issues);
    issues
}

/// Unknown or malformed escape sequences in a backtick string literal text
/// segment. Offsets are byte offsets into `input`.
pub fn backtick_string_literal_escape_issues(input: &str) -> Vec<EscapeIssue> {
    let mut issues = Vec::new();
    unescape_with(input, EscapeFlavor::Backtick, &mut issues);
    issues
}

/// An escape sequence that did not decode. The sequence is kept verbatim in
/// the decoded string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EscapeIssue {
    /// Byte offset of the backslash.
    pub start: usize,
    /// Byte length of the offending sequence, backslash included.
    pub len: usize,
    pub kind: EscapeIssueKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EscapeIssueKind {
    /// `\` followed by a character that is not an escape (e.g. `\q`).
    Unknown(char),
    /// `\x` not followed by exactly two hex digits.
    InvalidHex,
    /// `\u` not followed by four hex digits or `{1-6 hex digits}`.
    InvalidUnicode,
    /// `\u{...}` naming a value above U+10FFFF.
    OutOfRange,
    /// `\uHHHH` naming a surrogate that is not part of a valid pair.
    LoneSurrogate,
}

impl EscapeIssueKind {
    /// Malformed code-point escapes are errors; an unknown escape character is
    /// only suspicious (it has always been kept verbatim, e.g. regex `\d`).
    pub fn is_error(&self) -> bool {
        !matches!(self, EscapeIssueKind::Unknown(_))
    }
}

impl std::fmt::Display for EscapeIssueKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EscapeIssueKind::Unknown(c) => write!(
                f,
                "unknown escape sequence `\\{}`; it is kept as-is (write `\\\\{}` for a literal backslash)",
                c.escape_debug(),
                c.escape_debug()
            ),
            EscapeIssueKind::InvalidHex => {
                write!(f, "invalid `\\x` escape: expected exactly two hex digits")
            }
            EscapeIssueKind::InvalidUnicode => write!(
                f,
                "invalid `\\u` escape: expected `\\uHHHH` or `\\u{{H...}}` with 1-6 hex digits"
            ),
            EscapeIssueKind::OutOfRange => {
                write!(f, "invalid `\\u{{...}}` escape: value exceeds U+10FFFF")
            }
            EscapeIssueKind::LoneSurrogate => write!(
                f,
                "invalid `\\u` escape: unpaired UTF-16 surrogate; use `\\u{{...}}` for the full code point"
            ),
        }
    }
}

#[derive(Copy, Clone)]
enum EscapeFlavor {
    Quote,
    Backtick,
}

/// Parse exactly `n` hex digits at the start of `s`.
fn hex_prefix(s: &str, n: usize) -> Option<u32> {
    let digits = s.get(..n)?;
    if !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    u32::from_str_radix(digits, 16).ok()
}

/// Decode the code-point escape whose text (after the backslash) starts at
/// `rest`, which begins with `x` or `u`. Returns the decoded character and the
/// byte length consumed after the backslash.
fn decode_code_point_escape(rest: &str) -> Result<(char, usize), (EscapeIssueKind, usize)> {
    if let Some(after_x) = rest.strip_prefix('x') {
        return match hex_prefix(after_x, 2) {
            Some(v) => Ok((char::from_u32(v).expect("<= 0xFF is a scalar"), 3)),
            None => Err((EscapeIssueKind::InvalidHex, 1)),
        };
    }
    let after_u = rest.strip_prefix('u').expect("caller checked `u`");
    if let Some(braced) = after_u.strip_prefix('{') {
        let Some(close) = braced.find('}') else {
            return Err((EscapeIssueKind::InvalidUnicode, 1));
        };
        let digits = &braced[..close];
        let consumed = 3 + close; // `u{` + digits + `}`
        if digits.is_empty() || digits.len() > 6 || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err((EscapeIssueKind::InvalidUnicode, consumed));
        }
        let v = u32::from_str_radix(digits, 16).expect("validated hex");
        return match char::from_u32(v) {
            Some(c) => Ok((c, consumed)),
            None if v > 0x10_FFFF => Err((EscapeIssueKind::OutOfRange, consumed)),
            None => Err((EscapeIssueKind::LoneSurrogate, consumed)),
        };
    }
    let Some(v) = hex_prefix(after_u, 4) else {
        return Err((EscapeIssueKind::InvalidUnicode, 1));
    };
    if let Some(c) = char::from_u32(v) {
        return Ok((c, 5));
    }
    // Surrogate: only a high surrogate followed by `\uDC00..=\uDFFF` is valid.
    if (0xD800..0xDC00).contains(&v) {
        if let Some(low) = after_u[4..]
            .strip_prefix("\\u")
            .and_then(|s| hex_prefix(s, 4))
            .filter(|lo| (0xDC00..0xE000).contains(lo))
        {
            let cp = 0x1_0000 + ((v - 0xD800) << 10) + (low - 0xDC00);
            return Ok((char::from_u32(cp).expect("valid surrogate pair"), 11));
        }
    }
    Err((EscapeIssueKind::LoneSurrogate, 5))
}

fn unescape_with(input: &str, flavor: EscapeFlavor, issues: &mut Vec<EscapeIssue>) -> String {
    let mut result = String::with_capacity(input.len());
    let mut chars = input.char_indices().peekable();
    while let Some((start, c)) = chars.next() {
        // BEP-049 §AA (TS parity): normalize line endings in backtick
        // literal text. `\r\n` → `\n`, lone `\r` → `\n`. Mirrors
        // typescript-go's scanner (scanner.go:1650-1660).
        if matches!(flavor, EscapeFlavor::Backtick) && c == '\r' {
            // Consume an immediately-following `\n` (the CRLF case).
            if chars.peek().map(|&(_, c)| c) == Some('\n') {
                chars.next();
            }
            result.push('\n');
            continue;
        }
        if c != '\\' {
            result.push(c);
            continue;
        }
        match chars.next().map(|(_, c)| c) {
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
            Some('\'') => result.push('\''),
            Some('`') if matches!(flavor, EscapeFlavor::Backtick) => result.push('`'),
            Some('$') if matches!(flavor, EscapeFlavor::Backtick) => result.push('$'),
            Some('x' | 'u') => {
                let rest = &input[start + 1..];
                let (consumed, decoded) = match decode_code_point_escape(rest) {
                    Ok((ch, consumed)) => (consumed, Some(ch)),
                    Err((kind, consumed)) => {
                        issues.push(EscapeIssue {
                            start,
                            len: 1 + consumed,
                            kind,
                        });
                        // Keep only the `\x`/`\u` verbatim; the rest is
                        // re-scanned as ordinary text.
                        (1, None)
                    }
                };
                match decoded {
                    Some(ch) => result.push(ch),
                    None => result.push_str(&input[start..start + 2]),
                }
                // `chars` already consumed the `x`/`u`; skip the remainder.
                let end = start + 1 + consumed;
                while chars.peek().is_some_and(|&(i, _)| i < end) {
                    chars.next();
                }
            }
            Some(other) => {
                issues.push(EscapeIssue {
                    start,
                    len: 1 + other.len_utf8(),
                    kind: EscapeIssueKind::Unknown(other),
                });
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
        assert_eq!(unescape_string_literal(r"\q"), "\\q");
        assert_eq!(unescape_string_literal(r"\d+"), "\\d+");
        assert_eq!(
            string_literal_escape_issues(r"a\qb"),
            vec![EscapeIssue {
                start: 1,
                len: 2,
                kind: EscapeIssueKind::Unknown('q')
            }]
        );
    }

    #[test]
    fn quote_decodes_hex_escapes() {
        assert_eq!(unescape_string_literal(r"a\x41b"), "aAb");
        assert_eq!(unescape_string_literal(r"\x1b[0m"), "\u{1b}[0m");
        assert_eq!(unescape_string_literal(r"\xff"), "\u{ff}");
        assert!(string_literal_escape_issues(r"\x41\x1B").is_empty());
    }

    #[test]
    fn quote_decodes_unicode_escapes() {
        assert_eq!(unescape_string_literal(r"a\u0041b"), "aAb");
        assert_eq!(unescape_string_literal(r"\u00e9"), "\u{e9}");
        assert_eq!(unescape_string_literal(r"\u{1F600}"), "\u{1F600}");
        assert_eq!(unescape_string_literal(r"\u{41}\u{0}"), "A\0");
        assert_eq!(unescape_string_literal(r"\u{10FFFF}"), "\u{10FFFF}");
        // UTF-16 surrogate pair (JavaScript parity).
        assert_eq!(unescape_string_literal(r"\uD83D\uDE00"), "\u{1F600}");
        assert!(string_literal_escape_issues(r"\u0041\u{1F600}\uD83D\uDE00").is_empty());
    }

    #[test]
    fn quote_decodes_single_quote_escape() {
        assert_eq!(unescape_string_literal(r"it\'s"), "it's");
    }

    #[test]
    fn malformed_code_point_escapes_are_kept_and_reported() {
        let cases: &[(&str, &str, EscapeIssueKind, usize, usize)] = &[
            (r"\x4", r"\x4", EscapeIssueKind::InvalidHex, 0, 2),
            (r"\xZZ", r"\xZZ", EscapeIssueKind::InvalidHex, 0, 2),
            (r"\u12", r"\u12", EscapeIssueKind::InvalidUnicode, 0, 2),
            (r"\u{}", r"\u{}", EscapeIssueKind::InvalidUnicode, 0, 4),
            (
                r"\u{1234567}",
                r"\u{1234567}",
                EscapeIssueKind::InvalidUnicode,
                0,
                11,
            ),
            (r"\u{12", r"\u{12", EscapeIssueKind::InvalidUnicode, 0, 2),
            (
                r"\u{110000}",
                r"\u{110000}",
                EscapeIssueKind::OutOfRange,
                0,
                10,
            ),
            (r"\uD800", r"\uD800", EscapeIssueKind::LoneSurrogate, 0, 6),
            (r"\uDE00x", r"\uDE00x", EscapeIssueKind::LoneSurrogate, 0, 6),
            (
                r"\u{D800}",
                r"\u{D800}",
                EscapeIssueKind::LoneSurrogate,
                0,
                8,
            ),
            (r"ab\xq", r"ab\xq", EscapeIssueKind::InvalidHex, 2, 2),
        ];
        for (input, decoded, kind, start, len) in cases {
            assert_eq!(&unescape_string_literal(input), decoded, "{input}");
            let issues = string_literal_escape_issues(input);
            assert_eq!(
                issues,
                vec![EscapeIssue {
                    start: *start,
                    len: *len,
                    kind: kind.clone()
                }],
                "{input}"
            );
            assert!(kind.is_error());
        }
        assert!(!EscapeIssueKind::Unknown('q').is_error());
    }

    #[test]
    fn issue_offsets_are_byte_offsets() {
        let issues = string_literal_escape_issues("é\\q\\x");
        assert_eq!(issues.len(), 2);
        assert_eq!((issues[0].start, issues[0].len), (2, 2));
        assert_eq!((issues[1].start, issues[1].len), (4, 2));
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
        assert_eq!(unescape_backtick_string_literal(r"\q"), "\\q");
        assert_eq!(backtick_string_literal_escape_issues(r"\q").len(), 1);
    }

    #[test]
    fn backtick_decodes_code_point_escapes() {
        assert_eq!(
            unescape_backtick_string_literal(r"\x41\u0042\u{1F600}"),
            "AB\u{1F600}"
        );
        assert!(backtick_string_literal_escape_issues(r"\x41\u0042\u{1F600}\`\$").is_empty());
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
