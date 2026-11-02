//! The lexer, built on [`logos`].
//!
//! [`enum@TokenKind`] is itself the Logos token enum: its `#[token]`/`#[regex]`
//! attributes are the code-mode rules, and the callbacks in this module scan
//! the bodies whose extent is not regular (strings, comments, hash strings).
//! A driver keeps the mode stack — a backtick opener switches to template
//! text, `${` switches back to code until its matching `}` — and `morph`s the
//! lexer into the private `Template` scanner and back at each switch, so no
//! token is ever lexed twice (a body callback may look ahead inside its own
//! literal, as the hash string's closer search does).
//!
//! The lexer has no diagnostics. Every byte of the input lands in exactly one
//! token, so `concat(tokens) == text` always holds, and what could not be
//! lexed as intended is visible in the kinds (`Error`, `Unclosed*`, a template
//! with no `BacktickEnd`); the parser reports them.

use logos::{Lexer, Logos};
use text_size::TextSize;

use crate::token::{TokenKind, Tokens};

#[cfg(test)]
mod tests;

fn size(pos: usize) -> TextSize {
    TextSize::try_from(pos).unwrap_or_else(|_| unreachable!("a source file is under 4 GiB"))
}

fn char_len(text: &str, at: usize) -> usize {
    text[at..].chars().next().map_or(1, char::len_utf8)
}

// ---------------------------------------------------------------------------
// Callbacks: bodies whose extent is not a regular expression
// ---------------------------------------------------------------------------

/// The rest of the line after a comment opener, not including the newline
/// (nor the `\r` of a CRLF).
fn to_line_end(lex: &mut Lexer<TokenKind>) {
    let rest = lex.remainder();
    let end = rest.find('\n').map_or(rest.len(), |nl| {
        nl - usize::from(rest[..nl].ends_with('\r'))
    });
    lex.bump(end);
}

/// A `//` comment, classified by what follows the slashes: `///` doc, `//#`
/// header, `//baml:` directive, anything else a plain line comment. One rule
/// with a callback instead of four `#[token]`s: Logos 0.15 miscompiles the
/// chain `/` < `//` < `///` beside `//baml:` (a `//bam…` that fails the
/// directive backtracks to `//` but reports `/`).
pub(crate) fn line_comment(lex: &mut Lexer<TokenKind>) -> TokenKind {
    let rest = lex.remainder();
    let kind = if rest.starts_with('/') {
        TokenKind::DocComment
    } else if rest.starts_with('#') {
        TokenKind::HeaderComment
    } else if rest.starts_with("baml:") {
        TokenKind::DirectiveComment
    } else {
        TokenKind::LineComment
    };
    to_line_end(lex);
    kind
}

/// Not nested; closes at the first `*/`. Unterminated, it runs to the end of
/// the text as `UnclosedBlockComment`.
pub(crate) fn block_comment(lex: &mut Lexer<TokenKind>) -> Result<(), TokenKind> {
    let rest = lex.remainder();
    match rest.find("*/") {
        Some(close) => {
            lex.bump(close + 2);
            Ok(())
        }
        None => {
            lex.bump(rest.len());
            Err(TokenKind::UnclosedBlockComment)
        }
    }
}

/// The body of `"…"`, `r"…"` or `b"…"` after its opener: `\` escapes the next
/// character, where a CRLF or a run of blanks counts as one. Unterminated: the token ends after the last
/// non-blank character so trailing whitespace stays trivia, and the token's
/// kind becomes the matching `Unclosed*` (Logos's error value).
pub(crate) fn quoted_body(lex: &mut Lexer<TokenKind>) -> Result<(), TokenKind> {
    let unclosed = match lex.slice() {
        "\"" => TokenKind::UnclosedString,
        "r\"" => TokenKind::UnclosedRawString,
        "b\"" => TokenKind::UnclosedByteString,
        opener => {
            unreachable!("`quoted_body` is attached to the three quote openers, not {opener:?}")
        }
    };
    let rest = lex.remainder();
    let bytes = rest.as_bytes();
    let mut i = 0;
    let mut content_end = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => {
                lex.bump(i + 1);
                return Ok(());
            }
            b'\\' => {
                i += 1;
                if i < bytes.len() {
                    if bytes[i..].starts_with(b"\r\n") {
                        i += 2;
                    } else if matches!(bytes[i], b' ' | b'\t') {
                        while i < bytes.len() && matches!(bytes[i], b' ' | b'\t') {
                            i += 1;
                        }
                    } else {
                        i += char_len(rest, i);
                    }
                }
                content_end = i;
            }
            b' ' | b'\t' | b'\n' | b'\r' => i += 1,
            _ => {
                i += char_len(rest, i);
                content_end = i;
            }
        }
    }
    lex.bump(content_end);
    Err(unclosed)
}

/// A `#`: a `#!` shebang line, the opener of a legacy `#"…"#` string (more
/// `#`s separated by blanks, then `"`), or a stray [`TokenKind::Hash`].
pub(crate) fn hash(lex: &mut Lexer<TokenKind>) -> TokenKind {
    let rest = lex.remainder();
    if rest.starts_with('!') {
        to_line_end(lex);
        return TokenKind::LineComment;
    }
    let bytes = rest.as_bytes();
    let mut i = 0;
    while bytes
        .get(i)
        .is_some_and(|b| matches!(b, b'#' | b' ' | b'\t' | b'\r' | b'\n'))
    {
        i += 1;
    }
    if bytes.get(i) == Some(&b'"') {
        lex.bump(i + 1);
        return match hash_body(lex) {
            Ok(()) => TokenKind::LegacyHashString,
            Err(unclosed) => unclosed,
        };
    }
    TokenKind::Hash
}

/// The body of a legacy `#…#"…"#…#` string after its opener: raw (no
/// escapes), closing at a `"` followed by exactly as many `#` as the opener,
/// blanks and newlines allowed between them.
fn hash_body(lex: &mut Lexer<TokenKind>) -> Result<(), TokenKind> {
    let hashes = lex.slice().bytes().filter(|&b| b == b'#').count();
    let rest = lex.remainder();
    let bytes = rest.as_bytes();
    let mut i = 0;
    let mut content_end = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => {
                let mut after = i + 1;
                let mut count = 0;
                let mut last_hash = None;
                while let Some(&b) = bytes.get(after) {
                    match b {
                        b'#' => {
                            count += 1;
                            last_hash = Some(after);
                        }
                        b' ' | b'\t' | b'\n' | b'\r' => {}
                        _ => break,
                    }
                    after += 1;
                }
                if count == hashes
                    && let Some(last) = last_hash
                {
                    lex.bump(last + 1);
                    return Ok(());
                }
                i += 1;
                content_end = i;
            }
            b' ' | b'\t' | b'\n' | b'\r' => i += 1,
            _ => {
                i += char_len(rest, i);
                content_end = i;
            }
        }
    }
    lex.bump(content_end);
    Err(TokenKind::UnclosedHashString)
}

// ---------------------------------------------------------------------------
// Template text
// ---------------------------------------------------------------------------

/// The scanner for the text inside a backtick string. Not vocabulary: the
/// driver folds its matches into [`TokenKind::TemplateChunk`],
/// [`TokenKind::DollarLBrace`] and [`TokenKind::BacktickEnd`].
#[derive(Logos, Debug, Clone, Copy, PartialEq, Eq)]
enum Template {
    #[regex("`+")]
    Backticks,
    /// `$` immediately followed by `{` — nothing between.
    #[token("${")]
    Interpolation,
    /// `\` and the one character it escapes (a CRLF counts as one), or a
    /// trailing `\` at the end of the input.
    #[regex(r"\\(\r\n|\n|.)")]
    #[token("\\")]
    Escape,
    /// A lone `$`, or a run of anything that is not a tick, `$` or `\`.
    #[token("$")]
    #[regex(r"[^`$\\]+")]
    Text,
}

// ---------------------------------------------------------------------------
// Driver
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
enum Mode {
    /// Ordinary code. `depth` counts unmatched `{`; at depth 0 inside an
    /// interpolation, `}` ends the interpolation.
    Code { depth: u32 },
    /// Inside a backtick template opened by `ticks` backticks.
    Template { ticks: usize },
}

/// The token arrays under construction. Kinds and starts grow together and
/// become a [`Tokens`] in one step once the sentinel is known.
struct Out {
    kinds: Vec<TokenKind>,
    starts: Vec<TextSize>,
}

impl Out {
    fn push(&mut self, kind: TokenKind, start: usize) {
        self.kinds.push(kind);
        self.starts.push(size(start));
    }

    fn finish(mut self, len: usize) -> Tokens {
        self.starts.push(size(len));
        Tokens::from_parts(self.kinds, self.starts)
    }
}

/// Lex `text` completely.
pub fn lex(text: &str) -> Tokens {
    let mut out = Out {
        kinds: Vec::with_capacity(text.len() / 4),
        starts: Vec::with_capacity(text.len() / 4 + 1),
    };
    let mut modes = vec![Mode::Code { depth: 0 }];
    let mut code = TokenKind::lexer(text);
    'code: while let Some(step) = code.next() {
        let start = code.span().start;
        // A failed match is `Err(Error)`; an unclosed literal is `Err(Unclosed*)`.
        let kind = match step {
            Ok(kind) | Err(kind) => kind,
        };
        match kind {
            TokenKind::BacktickStart => {
                out.push(TokenKind::BacktickStart, start);
                modes.push(Mode::Template {
                    ticks: code.span().len(),
                });
                // Switch to template text; `morph` keeps the position.
                let mut template: Lexer<Template> = code.morph();
                let resume = run_template(&mut template, &mut modes, &mut out);
                code = template.morph();
                if let Resume::End = resume {
                    break 'code;
                }
            }
            TokenKind::LBrace => {
                out.push(TokenKind::LBrace, start);
                let Some(Mode::Code { depth }) = modes.last_mut() else {
                    unreachable!("code tokens are only lexed in code mode")
                };
                *depth += 1;
            }
            TokenKind::RBrace => {
                out.push(TokenKind::RBrace, start);
                let Some(Mode::Code { depth }) = modes.last_mut() else {
                    unreachable!("code tokens are only lexed in code mode")
                };
                if *depth > 0 {
                    *depth -= 1;
                } else if modes.len() > 1 {
                    // The `}` closing a `${…}` interpolation: back to template text.
                    modes.pop();
                    let mut template: Lexer<Template> = code.morph();
                    let resume = run_template(&mut template, &mut modes, &mut out);
                    code = template.morph();
                    if let Resume::End = resume {
                        break 'code;
                    }
                }
            }
            kind => out.push(kind, start),
        }
    }
    out.finish(text.len())
}

/// Why `run_template` handed control back.
enum Resume {
    /// Code resumes: an interpolation opened, or the template closed.
    Code,
    /// The input ended inside the template.
    End,
}

/// Lex template text until `${` (push a code mode and return), the closing
/// run (pop and return) or the end of input. Consecutive text matches become
/// one `TemplateChunk`.
fn run_template(lexer: &mut Lexer<Template>, modes: &mut Vec<Mode>, out: &mut Out) -> Resume {
    let Some(Mode::Template { ticks }) = modes.last().copied() else {
        unreachable!("run_template is entered in template mode")
    };
    let mut chunk_start: Option<usize> = None;
    loop {
        let Some(step) = lexer.next() else {
            if let Some(start) = chunk_start {
                out.push(TokenKind::TemplateChunk, start);
            }
            return Resume::End;
        };
        let span = lexer.span();
        match step {
            Err(()) | Ok(Template::Text | Template::Escape) => {
                chunk_start.get_or_insert(span.start);
            }
            Ok(Template::Interpolation) => {
                if let Some(start) = chunk_start {
                    out.push(TokenKind::TemplateChunk, start);
                }
                out.push(TokenKind::DollarLBrace, span.start);
                modes.push(Mode::Code { depth: 0 });
                return Resume::Code;
            }
            Ok(Template::Backticks) => {
                if span.len() < ticks {
                    chunk_start.get_or_insert(span.start);
                    continue;
                }
                // Anchored close: the excess leading ticks are content.
                let close_start = span.end - ticks;
                if close_start > span.start {
                    chunk_start.get_or_insert(span.start);
                }
                if let Some(start) = chunk_start {
                    out.push(TokenKind::TemplateChunk, start);
                }
                out.push(TokenKind::BacktickEnd, close_start);
                modes.pop();
                return Resume::Code;
            }
        }
    }
}
