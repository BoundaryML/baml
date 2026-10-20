//! String-literal escape decoding.
//!
//! Single source of truth for escape sequences across the BAML compiler. Used
//! by regular `"..."` literals, BEP-049 backtick `` `...` `` literals, and
//! `b"..."` byte string literals. Raw `r"..."` literals process no escapes.
//!
//! Recognized escapes (every literal):
//! - `\\`, `\"`, `\'`, `` \` ``, `\$`, `\/`
//! - `\n`, `\t`, `\r`, `\b` (U+0008), `\f` (U+000C), `\0` (always NUL; BAML
//!   has no octal escapes, so `\01` is NUL followed by `1`)
//! - `\xHH`: exactly two hex digits. Strings accept `\x00`-`\x7F` only (Rust's
//!   rule); byte strings accept any byte value.
//! - `\uHHHH` and `\u{H...}` (strings only): `\u{...}` takes 1-6 hex digits,
//!   with `_` allowed between digits. A high surrogate immediately followed by
//!   a low surrogate, written in either form, combines into one code point
//!   (`\uD83D\uDE00` is U+1F600), so JSON string bodies can be pasted as-is.
//!   An unpaired surrogate is invalid.
//!
//! `\$` covers the `\${` disambiguation from §8 of BEP-049: backslash before
//! `$` always produces a literal `$`, so `\${name}` renders as `${name}`.
//!
//! Anything else after a backslash (`\d`, `\v`, `\U0001F600`, a backslash at
//! the end of a line, ...) is an [`EscapeIssue`]. Decoding is lenient: an
//! offending sequence is kept verbatim in the decoded value, and the compiler
//! reports it through the `*_escape_issues` functions, which run the same
//! scanner.

use std::fmt;

/// Decode escapes for a regular `"..."` string literal body (i.e., the text
/// between the surrounding quotes, with quotes already stripped).
pub fn unescape_string_literal(input: &str) -> String {
    decode_str(input, LiteralKind::Quoted, &mut Vec::new())
}

/// Decode escapes for a BEP-049 backtick string literal body (i.e., the text
/// between the surrounding backtick runs, with delimiters already stripped).
pub fn unescape_backtick_string_literal(input: &str) -> String {
    decode_str(input, LiteralKind::Backtick, &mut Vec::new())
}

/// Decode escapes for a `b"..."` byte string literal body (the text between
/// `b"` and `"`).
pub fn unescape_byte_string_literal(input: &str) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(input.len());
    scan(
        input,
        LiteralKind::Bytes,
        |piece| match piece {
            Piece::Char(c) => {
                let mut buf = [0; 4];
                bytes.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
            }
            Piece::Byte(b) => bytes.push(b),
            Piece::Verbatim(s) => bytes.extend_from_slice(s.as_bytes()),
        },
        &mut Vec::new(),
    );
    bytes
}

/// Invalid escape sequences in a regular `"..."` string literal body.
/// Offsets are byte offsets into `input`.
pub fn string_literal_escape_issues(input: &str) -> Vec<EscapeIssue> {
    let mut issues = Vec::new();
    scan(input, LiteralKind::Quoted, |_| {}, &mut issues);
    issues
}

/// Invalid escape sequences in one literal-text run of a backtick string
/// literal. Offsets are byte offsets into `input`.
pub fn backtick_string_literal_escape_issues(input: &str) -> Vec<EscapeIssue> {
    let mut issues = Vec::new();
    scan(input, LiteralKind::Backtick, |_| {}, &mut issues);
    issues
}

/// Invalid escape sequences and non-ASCII characters in a `b"..."` byte
/// string literal body. Offsets are byte offsets into `input`.
pub fn byte_string_literal_escape_issues(input: &str) -> Vec<EscapeIssue> {
    let mut issues = Vec::new();
    scan(input, LiteralKind::Bytes, |_| {}, &mut issues);
    issues
}

/// A sequence in a literal body that does not decode. The decoded value keeps
/// it verbatim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EscapeIssue {
    /// Byte offset of the start of the sequence (normally the backslash).
    pub start: usize,
    /// Byte length of the offending sequence.
    pub len: usize,
    /// The offending source text, `len` bytes from `start`.
    pub text: String,
    pub kind: EscapeIssueKind,
    /// The kind of literal the sequence appears in, so messages can quote it
    /// in the same syntax.
    pub literal: LiteralKind,
}

/// The kind of literal an [`EscapeIssue`] comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiteralKind {
    /// `"..."`
    Quoted,
    /// `` `...` ``
    Backtick,
    /// `b"..."`
    Bytes,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EscapeIssueKind {
    /// `\` followed by a character that is not an escape (e.g. `\d`, `\v`).
    Unknown(char),
    /// `\UHHHHHHHH` (Python's long Unicode escape).
    LongUnicode,
    /// `\N{name}` (Python's named Unicode escape).
    NamedUnicode,
    /// `\x` not followed by two hex digits.
    InvalidHex,
    /// `\x80`-`\xFF` outside a byte string.
    NonAsciiHex(u8),
    /// A malformed `\u` escape.
    InvalidUnicode(UnicodeEscapeError),
    /// `\u{...}` naming a value above U+10FFFF.
    OutOfRange,
    /// A surrogate code point that is not half of a high/low pair.
    LoneSurrogate(u32),
    /// A `\u` escape in a byte string. `utf8` holds the code point's UTF-8
    /// encoding when the escape is otherwise valid.
    UnicodeInByteString { utf8: Option<Vec<u8>> },
    /// A literal non-ASCII character in a byte string.
    NonAsciiInByteString(char),
}

/// Why a `\u` escape is malformed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnicodeEscapeError {
    /// `\u` followed by neither four hex digits nor `{`.
    Malformed,
    /// `\u{` without a closing `}`.
    Unterminated,
    /// A character other than a hex digit or `_` inside `\u{...}`.
    InvalidChar(char),
    /// `\u{}`.
    Empty,
    /// More than six hex digits inside `\u{...}`.
    TooLong,
    /// `_` at the start or end of the digits in `\u{...}`.
    MisplacedUnderscore,
}

impl EscapeIssueKind {
    /// A short label for the offending span.
    pub fn label(&self) -> &'static str {
        match self {
            EscapeIssueKind::Unknown(_) => "unknown escape",
            EscapeIssueKind::LongUnicode | EscapeIssueKind::NamedUnicode => "unsupported escape",
            EscapeIssueKind::NonAsciiInByteString(_) => "non-ASCII character",
            _ => "invalid escape",
        }
    }
}

impl LiteralKind {
    /// `body` spelled as a literal of this kind, so a message quotes source
    /// text in the same syntax the user wrote it in.
    fn quote(self, body: &str) -> String {
        match self {
            LiteralKind::Quoted => format!("\"{body}\""),
            LiteralKind::Bytes => format!("b\"{body}\""),
            LiteralKind::Backtick if body.contains('`') => format!("`` {body} ``"),
            LiteralKind::Backtick => format!("`{body}`"),
        }
    }
}

impl fmt::Display for EscapeIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let q = |body: &str| self.literal.quote(body);
        let text = q(&self.text);
        match &self.kind {
            EscapeIssueKind::Unknown(c) if c.is_whitespace() || c.is_control() => {
                write!(
                    f,
                    "unknown escape sequence: {} followed by {} - did you mean {} (escaped backslash)?",
                    q("\\"),
                    describe_char(*c),
                    q("\\\\"),
                )?;
                if matches!(c, '\n' | '\r') {
                    write!(f, " BAML has no line continuations.")?;
                }
                Ok(())
            }
            EscapeIssueKind::Unknown(c) => {
                write!(
                    f,
                    "unknown escape sequence {text} - did you mean {} (escaped backslash)",
                    q(&format!("\\\\{c}")),
                )?;
                // Raw strings exist only in the `r"..."` form.
                if self.literal == LiteralKind::Quoted {
                    write!(f, " or r\"\\{c}\" (raw string)")?;
                }
                write!(f, "?")
            }
            EscapeIssueKind::LongUnicode | EscapeIssueKind::NamedUnicode => write!(
                f,
                "unsupported escape sequence {text} - use {} for a Unicode code point",
                q("\\u{...}"),
            ),
            EscapeIssueKind::InvalidHex => write!(
                f,
                "invalid escape sequence {text} - {} must be followed by exactly two hex digits",
                q("\\x"),
            ),
            EscapeIssueKind::NonAsciiHex(_) => write!(
                f,
                "out of range escape sequence {text} - {} escapes in strings only go up to {}; use {} for a non-ASCII character, or a byte string b\"...\" for raw bytes",
                q("\\x"),
                q("\\x7F"),
                q("\\u{...}"),
            ),
            EscapeIssueKind::InvalidUnicode(err) => {
                write!(f, "invalid escape sequence {text} - ")?;
                match err {
                    UnicodeEscapeError::Malformed => write!(
                        f,
                        "expected four hex digits ({}) or a braced code point ({})",
                        q("\\uHHHH"),
                        q("\\u{H...}"),
                    ),
                    UnicodeEscapeError::Unterminated => write!(f, "missing the closing brace"),
                    UnicodeEscapeError::InvalidChar(c) => {
                        write!(f, "{} is not a hex digit", describe_char(*c))
                    }
                    UnicodeEscapeError::Empty => write!(f, "expected at least one hex digit"),
                    UnicodeEscapeError::TooLong => {
                        write!(f, "at most six hex digits are allowed")
                    }
                    UnicodeEscapeError::MisplacedUnderscore => {
                        write!(f, "underscores are only allowed between hex digits")
                    }
                }
            }
            EscapeIssueKind::OutOfRange => write!(
                f,
                "invalid escape sequence {text} - the value exceeds U+10FFFF"
            ),
            EscapeIssueKind::LoneSurrogate(v) => write!(
                f,
                "invalid escape sequence {text} - unpaired UTF-16 surrogate U+{v:04X}; a high surrogate (U+D800-U+DBFF) must be immediately followed by a low surrogate (U+DC00-U+DFFF)"
            ),
            EscapeIssueKind::UnicodeInByteString { utf8 } => {
                write!(
                    f,
                    "invalid escape sequence {text} - byte strings can't contain Unicode escapes; "
                )?;
                match utf8 {
                    Some(bytes) => {
                        write!(f, "did you mean {} (UTF-8 bytes)?", q(&hex_bytes(bytes)))
                    }
                    None => write!(f, "write the UTF-8 bytes with {}", q("\\xHH")),
                }
            }
            EscapeIssueKind::NonAsciiInByteString(c) => {
                let mut buf = [0; 4];
                write!(
                    f,
                    "non-ASCII character {text} in byte string - did you mean {} (UTF-8 bytes)?",
                    q(&hex_bytes(c.encode_utf8(&mut buf).as_bytes())),
                )
            }
        }
    }
}

/// A readable name for `c` inside a diagnostic message.
fn describe_char(c: char) -> String {
    match c {
        '\n' => "a newline".to_string(),
        '\r' => "a carriage return".to_string(),
        '\t' => "a tab".to_string(),
        ' ' => "a space".to_string(),
        c if c.is_whitespace() || c.is_control() => format!("U+{:04X}", u32::from(c)),
        c => format!("'{c}'"),
    }
}

fn hex_bytes(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes.iter().fold(String::new(), |mut out, b| {
        let _ = write!(out, "\\x{b:02X}");
        out
    })
}

/// One unit of decoded output.
enum Piece<'a> {
    Char(char),
    /// A raw byte from a byte string `\xHH` escape.
    Byte(u8),
    /// Source text kept as-is because it did not decode.
    Verbatim(&'a str),
}

fn decode_str(input: &str, literal: LiteralKind, issues: &mut Vec<EscapeIssue>) -> String {
    let mut result = String::with_capacity(input.len());
    scan(
        input,
        literal,
        |piece| match piece {
            Piece::Char(c) => result.push(c),
            Piece::Byte(b) => result.push(char::from(b)),
            Piece::Verbatim(s) => result.push_str(s),
        },
        issues,
    );
    result
}

fn scan<'a>(
    input: &'a str,
    literal: LiteralKind,
    mut emit: impl FnMut(Piece<'a>),
    issues: &mut Vec<EscapeIssue>,
) {
    let mut i = 0;
    while let Some(c) = input[i..].chars().next() {
        if c != '\\' {
            // BEP-049 §AA (TS parity): normalize line endings in backtick
            // literal text. `\r\n` → `\n`, lone `\r` → `\n`. Mirrors
            // typescript-go's scanner (scanner.go:1650-1660).
            if literal == LiteralKind::Backtick && c == '\r' {
                i += if input[i + 1..].starts_with('\n') {
                    2
                } else {
                    1
                };
                emit(Piece::Char('\n'));
                continue;
            }
            if literal == LiteralKind::Bytes && !c.is_ascii() {
                issues.push(EscapeIssue {
                    start: i,
                    len: c.len_utf8(),
                    text: c.to_string(),
                    kind: EscapeIssueKind::NonAsciiInByteString(c),
                    literal,
                });
            }
            emit(Piece::Char(c));
            i += c.len_utf8();
            continue;
        }

        let len = match decode_escape(&input[i..], literal) {
            Ok((piece, len)) => {
                emit(piece);
                len
            }
            Err((kind, len)) => {
                issues.push(EscapeIssue {
                    start: i,
                    len,
                    text: input[i..i + len].to_string(),
                    kind,
                    literal,
                });
                emit(Piece::Verbatim(&input[i..i + len]));
                len
            }
        };
        i += len;
    }
}

/// Decode the escape at the start of `s` (which begins with `\`). Returns the
/// decoded piece, or the issue, along with the byte length of the sequence.
fn decode_escape(
    s: &str,
    literal: LiteralKind,
) -> Result<(Piece<'static>, usize), (EscapeIssueKind, usize)> {
    let rest = &s[1..];
    let Some(c) = rest.chars().next() else {
        // A trailing backslash can't occur in a well-formed literal: it would
        // have escaped the closing delimiter. Keep it as-is.
        return Ok((Piece::Char('\\'), 1));
    };
    let simple = match c {
        'n' => Some('\n'),
        't' => Some('\t'),
        'r' => Some('\r'),
        'b' => Some('\u{0008}'),
        'f' => Some('\u{000C}'),
        '0' => Some('\0'),
        '\\' | '"' | '\'' | '`' | '$' | '/' => Some(c),
        _ => None,
    };
    if let Some(decoded) = simple {
        return Ok((Piece::Char(decoded), 2));
    }
    match c {
        'x' => {
            let digits = hex_run(&rest[1..], 2);
            if digits < 2 {
                return Err((EscapeIssueKind::InvalidHex, 2 + digits));
            }
            let byte = u8::from_str_radix(&rest[1..3], 16).expect("validated hex");
            if literal == LiteralKind::Bytes {
                Ok((Piece::Byte(byte), 4))
            } else if byte.is_ascii() {
                Ok((Piece::Char(char::from(byte)), 4))
            } else {
                Err((EscapeIssueKind::NonAsciiHex(byte), 4))
            }
        }
        'u' => {
            let decoded = decode_unicode_escape(s);
            if literal == LiteralKind::Bytes {
                let len = match &decoded {
                    Ok((_, len)) | Err((_, len)) => *len,
                };
                let utf8 = decoded.ok().map(|(ch, _)| {
                    let mut buf = [0; 4];
                    ch.encode_utf8(&mut buf).as_bytes().to_vec()
                });
                return Err((EscapeIssueKind::UnicodeInByteString { utf8 }, len));
            }
            decoded.map(|(ch, len)| (Piece::Char(ch), len))
        }
        'U' => Err((EscapeIssueKind::LongUnicode, 2 + hex_run(&rest[1..], 8))),
        'N' => {
            let len = match rest[1..].strip_prefix('{').and_then(|b| b.find('}')) {
                Some(close) => 2 + 1 + close + 1,
                None => 2,
            };
            Err((EscapeIssueKind::NamedUnicode, len))
        }
        // A line break after the backslash would be a line continuation,
        // which BAML doesn't support. Point at the backslash alone so the
        // diagnostic stays on one line.
        '\n' | '\r' => Err((EscapeIssueKind::Unknown(c), 1)),
        _ => Err((EscapeIssueKind::Unknown(c), 1 + c.len_utf8())),
    }
}

/// Decode a `\u` escape at the start of `s`, pairing a high surrogate with an
/// immediately following low-surrogate `\u` escape.
fn decode_unicode_escape(s: &str) -> Result<(char, usize), (EscapeIssueKind, usize)> {
    let (value, len) = unicode_escape_value(s)?;
    if let Some(ch) = char::from_u32(value) {
        return Ok((ch, len));
    }
    if value > 0x10_FFFF {
        return Err((EscapeIssueKind::OutOfRange, len));
    }
    if (0xD800..0xDC00).contains(&value)
        && s[len..].starts_with("\\u")
        && let Ok((low, low_len)) = unicode_escape_value(&s[len..])
        && (0xDC00..0xE000).contains(&low)
    {
        let cp = 0x1_0000 + ((value - 0xD800) << 10) + (low - 0xDC00);
        return Ok((
            char::from_u32(cp).expect("valid surrogate pair"),
            len + low_len,
        ));
    }
    Err((EscapeIssueKind::LoneSurrogate(value), len))
}

/// Parse the raw value of a `\uHHHH` or `\u{H...}` escape at the start of `s`
/// (which begins with `\u`), without checking that it is a scalar value.
fn unicode_escape_value(s: &str) -> Result<(u32, usize), (EscapeIssueKind, usize)> {
    let invalid = |err, len| Err((EscapeIssueKind::InvalidUnicode(err), len));
    let after_u = &s[2..];
    let Some(braced) = after_u.strip_prefix('{') else {
        let digits = hex_run(after_u, 4);
        if digits < 4 {
            return invalid(UnicodeEscapeError::Malformed, 2 + digits);
        }
        let value = u32::from_str_radix(&after_u[..4], 16).expect("validated hex");
        return Ok((value, 6));
    };

    let body_len = braced
        .bytes()
        .take_while(|b| b.is_ascii_hexdigit() || *b == b'_')
        .count();
    let body = &braced[..body_len];
    // `\u{` plus the body.
    let len = 3 + body_len;
    match braced[body_len..].chars().next() {
        Some('}') => {}
        Some(c) if c.is_alphanumeric() || c == '+' || c == '-' => {
            return invalid(UnicodeEscapeError::InvalidChar(c), len + c.len_utf8());
        }
        _ => return invalid(UnicodeEscapeError::Unterminated, len),
    }
    // Include the closing brace.
    let len = len + 1;
    if body.is_empty() {
        return invalid(UnicodeEscapeError::Empty, len);
    }
    if body.starts_with('_') || body.ends_with('_') {
        return invalid(UnicodeEscapeError::MisplacedUnderscore, len);
    }
    let digits: String = body.chars().filter(|&c| c != '_').collect();
    if digits.len() > 6 {
        return invalid(UnicodeEscapeError::TooLong, len);
    }
    let value = u32::from_str_radix(&digits, 16).expect("validated hex");
    Ok((value, len))
}

/// Byte length of the leading ASCII-hex-digit run of `s`, capped at `cap`.
fn hex_run(s: &str, cap: usize) -> usize {
    s.bytes()
        .take(cap)
        .take_while(u8::is_ascii_hexdigit)
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn issue_kinds(input: &str) -> Vec<EscapeIssueKind> {
        string_literal_escape_issues(input)
            .into_iter()
            .map(|issue| issue.kind)
            .collect()
    }

    fn issue_spans(input: &str) -> Vec<&str> {
        string_literal_escape_issues(input)
            .into_iter()
            .map(|issue| &input[issue.start..issue.start + issue.len])
            .collect()
    }

    #[test]
    fn decodes_simple_escapes() {
        assert_eq!(unescape_string_literal(r"line\nbreak"), "line\nbreak");
        assert_eq!(unescape_string_literal(r"tab\there"), "tab\there");
        assert_eq!(unescape_string_literal(r"cr\rhere"), "cr\rhere");
        assert_eq!(unescape_string_literal(r"nul\0here"), "nul\0here");
        assert_eq!(unescape_string_literal(r"back\\slash"), "back\\slash");
        assert_eq!(unescape_string_literal(r#"say \"hi\""#), "say \"hi\"");
        assert_eq!(unescape_string_literal(r"it\'s"), "it's");
        assert_eq!(unescape_string_literal(r"a\bb"), "a\u{0008}b");
        assert_eq!(unescape_string_literal(r"a\fb"), "a\u{000C}b");
        assert_eq!(unescape_string_literal(r"https:\/\/x.com"), "https://x.com");
        assert_eq!(unescape_string_literal(r"a\`b\$c"), "a`b$c");
        assert_eq!(issue_kinds(r#"\n\t\r\b\f\0\\\"\'\`\$\/"#), vec![]);
    }

    #[test]
    fn nul_is_never_octal() {
        assert_eq!(unescape_string_literal(r"\01"), "\u{0}1");
        assert_eq!(issue_kinds(r"\01"), vec![]);
        assert_eq!(issue_kinds(r"\101"), vec![EscapeIssueKind::Unknown('1')]);
    }

    #[test]
    fn decodes_ascii_hex_escapes() {
        assert_eq!(unescape_string_literal(r"\x41\x7f\x00"), "A\u{7F}\0");
        assert_eq!(issue_kinds(r"\x41\x7F"), vec![]);
    }

    #[test]
    fn rejects_non_ascii_or_malformed_hex_escapes() {
        assert_eq!(
            issue_kinds(r"\xE9"),
            vec![EscapeIssueKind::NonAsciiHex(0xE9)]
        );
        assert_eq!(issue_spans(r"a\xE9b"), vec![r"\xE9"]);
        assert_eq!(issue_kinds(r"\x4"), vec![EscapeIssueKind::InvalidHex]);
        assert_eq!(issue_spans(r"\x4g"), vec![r"\x4"]);
        assert_eq!(issue_spans(r"\x"), vec![r"\x"]);
    }

    #[test]
    fn decodes_unicode_escapes() {
        assert_eq!(unescape_string_literal(r"\u00E9"), "é");
        assert_eq!(unescape_string_literal(r"\u4E2D"), "中");
        assert_eq!(unescape_string_literal(r"\u{41}"), "A");
        assert_eq!(unescape_string_literal(r"\u{1F600}"), "😀");
        assert_eq!(unescape_string_literal(r"\u{10FFFF}"), "\u{10FFFF}");
        assert_eq!(unescape_string_literal(r"\u{1F_600}"), "😀");
        assert_eq!(unescape_string_literal(r"\u{4_1}"), "A");
        assert_eq!(unescape_string_literal(r"\u{00_0041}"), "A");
    }

    #[test]
    fn pairs_surrogates_in_any_form() {
        for pair in [
            r"\uD83D\uDE00",
            r"\uD83D\u{DE00}",
            r"\u{D83D}\uDE00",
            r"\u{D83D}\u{DE00}",
        ] {
            assert_eq!(unescape_string_literal(pair), "😀", "{pair}");
            assert_eq!(issue_kinds(pair), vec![], "{pair}");
        }
    }

    #[test]
    fn rejects_unpaired_surrogates() {
        assert_eq!(
            issue_kinds(r"\uD83D"),
            vec![EscapeIssueKind::LoneSurrogate(0xD83D)]
        );
        assert_eq!(
            issue_kinds(r"\uDE00"),
            vec![EscapeIssueKind::LoneSurrogate(0xDE00)]
        );
        assert_eq!(
            issue_kinds(r"\u{DE00}\uD83D"),
            vec![
                EscapeIssueKind::LoneSurrogate(0xDE00),
                EscapeIssueKind::LoneSurrogate(0xD83D)
            ]
        );
        // A high surrogate followed by something other than a low surrogate
        // reports only the high surrogate; the next escape decodes normally.
        assert_eq!(issue_spans(r"\uD83D\u0041"), vec![r"\uD83D"]);
        assert_eq!(unescape_string_literal(r"\uD83D\u0041"), "\\uD83DA");
        assert_eq!(issue_spans(r"\uD83Dx\uDE00"), vec![r"\uD83D", r"\uDE00"]);
    }

    #[test]
    fn rejects_malformed_unicode_escapes() {
        use UnicodeEscapeError::*;
        let invalid = EscapeIssueKind::InvalidUnicode;
        assert_eq!(issue_kinds(r"\u12"), vec![invalid(Malformed)]);
        assert_eq!(issue_spans(r"\u12x"), vec![r"\u12"]);
        assert_eq!(issue_kinds(r"\u{}"), vec![invalid(Empty)]);
        assert_eq!(issue_kinds(r"\u{41"), vec![invalid(Unterminated)]);
        assert_eq!(issue_spans(r"\u{41 x}"), vec![r"\u{41"]);
        assert_eq!(issue_kinds(r"\u{12g4}"), vec![invalid(InvalidChar('g'))]);
        assert_eq!(issue_spans(r"\u{12g4}"), vec![r"\u{12g"]);
        assert_eq!(issue_kinds(r"\u{0000041}"), vec![invalid(TooLong)]);
        assert_eq!(issue_kinds(r"\u{_41}"), vec![invalid(MisplacedUnderscore)]);
        assert_eq!(issue_kinds(r"\u{41_}"), vec![invalid(MisplacedUnderscore)]);
        assert_eq!(
            issue_kinds(r"\u{110000}"),
            vec![EscapeIssueKind::OutOfRange]
        );
    }

    #[test]
    fn rejects_python_unicode_escapes() {
        assert_eq!(
            issue_kinds(r"\U0001F600"),
            vec![EscapeIssueKind::LongUnicode]
        );
        assert_eq!(issue_spans(r"\U0001F600!"), vec![r"\U0001F600"]);
        assert_eq!(
            issue_kinds(r"\N{GRINNING FACE}"),
            vec![EscapeIssueKind::NamedUnicode]
        );
        assert_eq!(
            issue_spans(r"\N{GRINNING FACE}!"),
            vec![r"\N{GRINNING FACE}"]
        );
    }

    #[test]
    fn rejects_unknown_escapes() {
        for (input, c) in [
            (r"\d+", 'd'),
            (r"\v", 'v'),
            (r"\a", 'a'),
            (r"\e", 'e'),
            (r"\q", 'q'),
        ] {
            assert_eq!(
                issue_kinds(input),
                vec![EscapeIssueKind::Unknown(c)],
                "{input}"
            );
        }
        assert_eq!(issue_spans(r"a\d+b\é"), vec![r"\d", r"\é"]);
        // Unknown escapes are kept verbatim by the lenient decoder.
        assert_eq!(unescape_string_literal(r"\d+"), r"\d+");
    }

    #[test]
    fn rejects_line_continuation() {
        assert_eq!(issue_kinds("a\\\nb"), vec![EscapeIssueKind::Unknown('\n')]);
        // The span stops at the backslash so it stays on one line.
        assert_eq!(issue_spans("a\\\nb"), vec!["\\"]);
    }

    #[test]
    fn issue_offsets_are_byte_offsets() {
        let issues = string_literal_escape_issues(r"é\q");
        assert_eq!(issues[0].start, 2);
        assert_eq!(issues[0].len, 2);
    }

    #[test]
    fn handles_empty_plain_and_trailing_backslash() {
        assert_eq!(unescape_string_literal(""), "");
        assert_eq!(unescape_string_literal("plain text"), "plain text");
        assert_eq!(unescape_string_literal("trailing\\"), "trailing\\");
    }

    #[test]
    fn backtick_uses_the_same_escapes() {
        assert_eq!(
            unescape_backtick_string_literal(r"line\nbreak"),
            "line\nbreak"
        );
        assert_eq!(unescape_backtick_string_literal(r"a\`b"), "a`b");
        assert_eq!(unescape_backtick_string_literal(r"a\${x}b"), "a${x}b");
        assert_eq!(unescape_backtick_string_literal(r"\u{1F600}"), "😀");
        let issues: Vec<_> = backtick_string_literal_escape_issues(r"\d \v")
            .into_iter()
            .map(|issue| (issue.start, issue.text, issue.kind, issue.literal))
            .collect();
        assert_eq!(
            issues,
            vec![
                (
                    0,
                    r"\d".to_string(),
                    EscapeIssueKind::Unknown('d'),
                    LiteralKind::Backtick
                ),
                (
                    3,
                    r"\v".to_string(),
                    EscapeIssueKind::Unknown('v'),
                    LiteralKind::Backtick
                ),
            ]
        );
    }

    #[test]
    fn backtick_normalizes_line_endings() {
        // BEP-049 §AA / TypeScript-go scanner.go:1650-1660.
        assert_eq!(
            unescape_backtick_string_literal("line1\r\nline2"),
            "line1\nline2"
        );
        assert_eq!(
            unescape_backtick_string_literal("line1\rline2"),
            "line1\nline2"
        );
        assert_eq!(
            unescape_backtick_string_literal("a\r\nb\rc\nd"),
            "a\nb\nc\nd"
        );
    }

    #[test]
    fn quote_flavor_does_not_normalize_cr() {
        // The CR/CRLF normalization is backtick-specific (BEP-049 §12).
        assert_eq!(unescape_string_literal("a\r\nb"), "a\r\nb");
    }

    #[test]
    fn byte_strings_decode_escapes_and_any_hex_byte() {
        assert_eq!(unescape_byte_string_literal("GIF89a"), b"GIF89a");
        assert_eq!(unescape_byte_string_literal(r"a\nb"), b"a\nb");
        assert_eq!(
            unescape_byte_string_literal(r"\xFF\x00\xc3"),
            [0xFF, 0, 0xC3]
        );
        assert_eq!(
            unescape_byte_string_literal(r#"\'\b\f\/\`\$\\\""#),
            b"'\x08\x0C/`$\\\""
        );
        assert_eq!(
            byte_string_literal_escape_issues(r"\xFF\'\b\f\/\`\$"),
            vec![]
        );
    }

    #[test]
    fn byte_strings_reject_unicode_escapes_and_non_ascii() {
        let issues = byte_string_literal_escape_issues(r"\u{E9}");
        assert_eq!(
            issues[0].kind,
            EscapeIssueKind::UnicodeInByteString {
                utf8: Some(vec![0xC3, 0xA9])
            }
        );
        assert_eq!(issues[0].len, r"\u{E9}".len());
        assert_eq!(
            byte_string_literal_escape_issues(r"\u{}")[0].kind,
            EscapeIssueKind::UnicodeInByteString { utf8: None }
        );
        let issues = byte_string_literal_escape_issues("aé");
        assert_eq!(issues[0].kind, EscapeIssueKind::NonAsciiInByteString('é'));
        assert_eq!((issues[0].start, issues[0].len), (1, 2));
        assert_eq!(
            byte_string_literal_escape_issues(r"\d")[0].kind,
            EscapeIssueKind::Unknown('d')
        );
    }

    fn message(issues: &[EscapeIssue]) -> String {
        let [issue] = issues else {
            panic!("expected one issue, got {issues:?}");
        };
        issue.to_string()
    }

    #[test]
    fn messages_quote_in_the_literals_own_syntax() {
        assert_eq!(
            message(&string_literal_escape_issues(r"\d")),
            r#"unknown escape sequence "\d" - did you mean "\\d" (escaped backslash) or r"\d" (raw string)?"#
        );
        assert_eq!(
            message(&backtick_string_literal_escape_issues(r"\d")),
            r#"unknown escape sequence `\d` - did you mean `\\d` (escaped backslash)?"#
        );
        assert_eq!(
            message(&byte_string_literal_escape_issues(r"\d")),
            r#"unknown escape sequence b"\d" - did you mean b"\\d" (escaped backslash)?"#
        );
        assert_eq!(
            message(&string_literal_escape_issues("a\\\nb")),
            r#"unknown escape sequence: "\" followed by a newline - did you mean "\\" (escaped backslash)? BAML has no line continuations."#
        );
        assert_eq!(
            message(&string_literal_escape_issues(r"\U0001F600")),
            r#"unsupported escape sequence "\U0001F600" - use "\u{...}" for a Unicode code point"#
        );
        assert_eq!(
            message(&backtick_string_literal_escape_issues(r"\N{GRINNING FACE}")),
            r#"unsupported escape sequence `\N{GRINNING FACE}` - use `\u{...}` for a Unicode code point"#
        );
        assert_eq!(
            message(&byte_string_literal_escape_issues(r"\u{E9}")),
            r#"invalid escape sequence b"\u{E9}" - byte strings can't contain Unicode escapes; did you mean b"\xC3\xA9" (UTF-8 bytes)?"#
        );
        assert_eq!(
            message(&byte_string_literal_escape_issues("é")),
            r#"non-ASCII character b"é" in byte string - did you mean b"\xC3\xA9" (UTF-8 bytes)?"#
        );
    }
}
