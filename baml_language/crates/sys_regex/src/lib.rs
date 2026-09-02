//! Pattern compilation and matching behind `baml.regex`.
//!
//! Shared by two callers that must agree exactly:
//!
//! - `bex_vm`'s `baml.regex` builtins, which compile and run patterns at
//!   runtime.
//! - The compiler's TIR pass, which compiles a *constant* pattern argument at
//!   build time so a bad pattern is a source diagnostic instead of a runtime
//!   throw. Because both go through [`Program::compile`], a pattern the
//!   compiler accepts is one the runtime accepts, and the message and span the
//!   user reads are the same in both places.
//!
//! # Two engines, one API
//!
//! The default dialect is the `regex` crate, which has no backtracking:
//! matching time is bounded by the product of pattern and haystack size, so a
//! pattern from an untrusted source cannot turn into a denial of service. That
//! guarantee is what costs it lookaround, backreferences, and subroutine calls.
//!
//! `backtracking = true` selects `fancy-regex`, which supports all of those and
//! can take exponential time to say "no match". Which engine a [`Program`]
//! holds is invisible to every method on it.
//!
//! # Offsets
//!
//! Everything here reports **byte** offsets into a `&str`, the unit both
//! engines use. Callers that need codepoint offsets (BAML strings are indexed
//! by codepoint) convert with [`char_offsets`], which handles a whole batch of
//! offsets in one pass over the subject rather than re-counting from the start
//! for each one.

use std::sync::OnceLock;

/// Pattern that can never match, used for `word("")`.
///
/// An empty literal has no whole-word occurrence to find, and treating it as an
/// empty match at every position would make a blank entry in a correction list
/// rewrite the entire document.
const NEVER_MATCHES: &str = "[^\\s\\S]";

// =============================================================================
// Errors
// =============================================================================

/// Why a pattern was rejected. Mirrors `baml.regex.ErrorKind`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorKind {
    /// The pattern does not parse.
    Syntax,
    /// The pattern parses, but uses a construct the selected dialect does not
    /// support.
    Unsupported,
    /// The compiled program exceeds the engine's size limit.
    TooLarge,
}

impl ErrorKind {
    /// The `baml.regex.ErrorKind` variant name for this kind.
    #[must_use]
    pub const fn variant_name(self) -> &'static str {
        match self {
            ErrorKind::Syntax => "Syntax",
            ErrorKind::Unsupported => "Unsupported",
            ErrorKind::TooLarge => "TooLarge",
        }
    }
}

/// A rejected pattern, with the engine's own diagnostic and, where the engine
/// reports one, the byte span of the offending construct within the pattern.
#[derive(Clone, Debug)]
pub struct BuildError {
    pub kind: ErrorKind,
    pub message: String,
    pub span: Option<(usize, usize)>,
}

/// The engine ran out of budget partway through a search.
///
/// Only the backtracking engine can produce this. It is a resource-exhaustion
/// condition, not a "no match": reporting it as "no match" would silently hide
/// a match a longer search would have found.
#[derive(Clone, Copy, Debug)]
pub struct SearchAborted {
    /// Short phrase naming what ran out, for the caller's error message.
    pub reason: &'static str,
}

// =============================================================================
// Matches
// =============================================================================

/// A match, as byte spans into the subject.
///
/// `groups[0]` is the whole match; the rest are the numbered capture groups in
/// the order they open in the pattern. `None` means the group did not
/// participate.
#[derive(Clone, Debug)]
pub struct RawMatch {
    pub groups: Vec<Option<(usize, usize)>>,
}

impl RawMatch {
    /// Byte span of the whole match.
    #[must_use]
    pub fn span(&self) -> (usize, usize) {
        self.groups.first().copied().flatten().unwrap_or((0, 0))
    }
}

// =============================================================================
// Compiled program
// =============================================================================

/// One of the two backing engines. Both are immutable after construction and
/// `Send + Sync`, which is what lets a compiled pattern be shared freely.
enum Engine {
    Safe(Box<regex::Regex>),
    Backtracking(Box<fancy_regex::Regex>),
}

/// A compiled pattern.
///
/// `Debug` prints the source pattern and dialect; neither engine's compiled
/// program has a readable representation, and the pattern is the part a
/// diagnostic wants anyway.
pub struct Program {
    engine: Engine,
    /// `\A(?:pattern)\z` twin used by [`Program::find_exact`], built on first
    /// use.
    ///
    /// Anchoring has to happen in the pattern rather than by checking a match's
    /// span: leftmost-first semantics can pick a shorter match at position 0
    /// while a longer one covering the whole subject exists. `\A` / `\z` (not
    /// `^` / `$`) so `(?m)` cannot reinterpret them as line boundaries.
    ///
    /// `None` inside the `OnceLock` means the twin failed to build, which
    /// should be unreachable — the inner pattern already compiled. Treated as
    /// "no match" rather than a panic.
    anchored: OnceLock<Option<Engine>>,
    /// Capture group names by group number; `None` for an unnamed group.
    names: Vec<Option<String>>,
    pattern: String,
    backtracking: bool,
}

#[expect(
    clippy::missing_fields_in_debug,
    reason = "neither engine's compiled program has a readable representation"
)]
impl std::fmt::Debug for Program {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Program")
            .field("pattern", &self.pattern)
            .field("backtracking", &self.backtracking)
            .finish()
    }
}

impl Program {
    /// Compile `pattern`. `backtracking` selects the dialect.
    ///
    /// # Errors
    /// [`BuildError`] if the pattern is malformed, uses a construct outside the
    /// selected dialect, or compiles to a program past the engine's size limit.
    pub fn compile(pattern: &str, backtracking: bool) -> Result<Self, BuildError> {
        let engine = build_engine(pattern, backtracking)?;
        let names = capture_names(&engine);
        Ok(Program {
            engine,
            anchored: OnceLock::new(),
            names,
            pattern: pattern.to_owned(),
            backtracking,
        })
    }

    /// Compile the whole-word matcher for the literal text `literal`.
    ///
    /// The literal is escaped, so metacharacters in it match themselves. Always
    /// uses the default dialect.
    ///
    /// # Errors
    /// [`BuildError`] if the escaped literal still exceeds the engine's size
    /// limit. The returned error's `span` refers to the *built* pattern, which
    /// [`Program::word_pattern`] reproduces.
    pub fn word(literal: &str, ignore_case: bool) -> Result<Self, BuildError> {
        Self::compile(&Self::word_pattern(literal, ignore_case), false)
    }

    /// The pattern [`Program::word`] compiles for `literal`.
    ///
    /// `\b` asserts a boundary, which is only the right assertion when the
    /// literal's outermost character is itself a word character. For `"2.5%"` a
    /// trailing `\b` would demand a word character *after* the `%`, rejecting
    /// `"a 2.5% raise"` — exactly backwards. Where the outermost character is
    /// not a word character, `\B` is the assertion that means "the neighbour is
    /// not a word character either".
    #[must_use]
    pub fn word_pattern(literal: &str, ignore_case: bool) -> String {
        if literal.is_empty() {
            return NEVER_MATCHES.to_owned();
        }
        let leads_with_word = literal.chars().next().is_some_and(is_word_char);
        let ends_with_word = literal.chars().next_back().is_some_and(is_word_char);

        let mut pattern = String::new();
        if ignore_case {
            pattern.push_str("(?i)");
        }
        pattern.push_str(if leads_with_word { "\\b" } else { "\\B" });
        pattern.push_str(&escape(literal));
        pattern.push_str(if ends_with_word { "\\b" } else { "\\B" });
        pattern
    }

    /// The pattern this program was compiled from.
    #[must_use]
    pub fn pattern(&self) -> &str {
        &self.pattern
    }

    /// Capture group names by group number; `None` for an unnamed group.
    #[must_use]
    pub fn names(&self) -> &[Option<String>] {
        &self.names
    }

    /// Does the pattern match anywhere in `subject`?
    ///
    /// # Errors
    /// [`SearchAborted`] if a backtracking search exhausts its budget.
    pub fn is_match(&self, subject: &str) -> Result<bool, SearchAborted> {
        match &self.engine {
            Engine::Safe(re) => Ok(re.is_match(subject)),
            Engine::Backtracking(re) => re.is_match(subject).map_err(|err| runtime_abort(&err)),
        }
    }

    /// The leftmost match, if any.
    ///
    /// # Errors
    /// [`SearchAborted`] if a backtracking search exhausts its budget.
    pub fn find_first(&self, subject: &str) -> Result<Option<RawMatch>, SearchAborted> {
        find_first_with(&self.engine, subject)
    }

    /// The match covering all of `subject`, if any.
    ///
    /// # Errors
    /// [`SearchAborted`] if a backtracking search exhausts its budget.
    pub fn find_exact(&self, subject: &str) -> Result<Option<RawMatch>, SearchAborted> {
        match self.anchored() {
            Some(engine) => find_first_with(engine, subject),
            None => Ok(None),
        }
    }

    /// Every non-overlapping match, leftmost-first.
    ///
    /// # Errors
    /// [`SearchAborted`] if a backtracking search exhausts its budget.
    pub fn find_all(&self, subject: &str) -> Result<Vec<RawMatch>, SearchAborted> {
        match &self.engine {
            Engine::Safe(re) => Ok(re
                .captures_iter(subject)
                .map(|caps| raw_from_safe(&caps))
                .collect()),
            Engine::Backtracking(re) => re
                .captures_iter(subject)
                .map(|caps| {
                    caps.map(|caps| raw_from_fancy(&caps))
                        .map_err(|err| runtime_abort(&err))
                })
                .collect(),
        }
    }

    /// Replace matches with `template`, expanding `$0` / `$1` / `$name` /
    /// `${name}` / `$$`. `limit` of 0 replaces every match.
    ///
    /// Both engines already implement exactly this expansion, so the template
    /// is handed to them verbatim rather than re-implemented here.
    ///
    /// # Errors
    /// [`SearchAborted`] if a backtracking search exhausts its budget.
    pub fn replacen<'t>(
        &self,
        subject: &'t str,
        limit: usize,
        template: &str,
    ) -> Result<std::borrow::Cow<'t, str>, SearchAborted> {
        match &self.engine {
            Engine::Safe(re) => Ok(re.replacen(subject, limit, template)),
            Engine::Backtracking(re) => re
                .try_replacen(subject, limit, template)
                .map_err(|err| runtime_abort(&err)),
        }
    }

    /// Split `subject` around each match.
    ///
    /// Returns the byte spans of the pieces, in order: the segment before each
    /// match, then the text of each *participating* capture group in that match
    /// (so splitting on `(<<\d+>>)` keeps the markers), and finally the tail.
    /// Never empty, and empty segments are kept rather than dropped.
    ///
    /// # Errors
    /// [`SearchAborted`] if a backtracking search exhausts its budget.
    pub fn split(&self, subject: &str) -> Result<Vec<(usize, usize)>, SearchAborted> {
        let matches = self.find_all(subject)?;
        let mut pieces: Vec<(usize, usize)> = Vec::with_capacity(matches.len() * 2 + 1);
        let mut cut = 0usize;
        for raw in &matches {
            let Some((start, end)) = raw.groups.first().copied().flatten() else {
                continue;
            };
            pieces.push((cut, start));
            pieces.extend(raw.groups.iter().skip(1).flatten().copied());
            cut = end;
        }
        pieces.push((cut, subject.len()));
        Ok(pieces)
    }

    fn anchored(&self) -> Option<&Engine> {
        self.anchored
            .get_or_init(|| {
                let wrapped = format!("\\A(?:{})\\z", self.pattern);
                build_engine(&wrapped, self.backtracking).ok()
            })
            .as_ref()
    }
}

// =============================================================================
// Free helpers
// =============================================================================

/// Escape every metacharacter in `literal` so the result matches `literal`
/// itself when embedded in a pattern.
#[must_use]
pub fn escape(literal: &str) -> String {
    regex::escape(literal)
}

/// Whether `c` is a word character for the purpose of [`Program::word`]'s
/// boundaries: a Unicode letter, a number, or `_`.
fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Convert byte offsets into `s` to codepoint offsets, in one pass.
///
/// The offsets need not be sorted — the traversal order is chosen internally —
/// but each must land on a character boundary, which every offset an engine
/// reports does. The result is positional: `out[i]` corresponds to `bytes[i]`.
#[must_use]
pub fn char_offsets(s: &str, bytes: &[usize]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..bytes.len()).collect();
    order.sort_unstable_by_key(|&i| bytes[i]);

    let mut out = vec![0usize; bytes.len()];
    let mut chars = s.char_indices();
    let mut scanned_bytes = 0usize;
    let mut scanned_chars = 0usize;

    for i in order {
        let target = bytes[i];
        while scanned_bytes < target {
            match chars.next() {
                Some((offset, c)) => {
                    scanned_bytes = offset + c.len_utf8();
                    scanned_chars += 1;
                }
                None => {
                    scanned_bytes = s.len();
                    break;
                }
            }
        }
        out[i] = scanned_chars;
    }
    out
}

// =============================================================================
// Engine plumbing
// =============================================================================

fn build_engine(pattern: &str, backtracking: bool) -> Result<Engine, BuildError> {
    if backtracking {
        fancy_regex::Regex::new(pattern)
            .map(|re| Engine::Backtracking(Box::new(re)))
            .map_err(fancy_build_error)
    } else {
        match regex::Regex::new(pattern) {
            Ok(re) => Ok(Engine::Safe(Box::new(re))),
            Err(err) => Err(safe_build_error(pattern, &err)),
        }
    }
}

/// Classify a default-dialect rejection.
///
/// The `regex` crate does not distinguish "malformed" from "well-formed but
/// unsupported", so the pattern is offered to `fancy-regex`: if that accepts
/// it, the only thing wrong with it here is the dialect, and the user wants to
/// hear about `backtracking = true` rather than hunt for a typo.
fn safe_build_error(pattern: &str, err: &regex::Error) -> BuildError {
    if let regex::Error::CompiledTooBig(limit) = err {
        return BuildError {
            kind: ErrorKind::TooLarge,
            message: format!("compiled pattern exceeds the {limit}-byte size limit"),
            span: None,
        };
    }
    let reason = condense(&err.to_string());
    let (kind, message) = if fancy_regex::Regex::new(pattern).is_ok() {
        (
            ErrorKind::Unsupported,
            format!(
                "{reason}; `backtracking = true` enables lookahead, lookbehind, \
                 backreferences, and subroutine calls, at the cost of the default \
                 dialect's matching-time bound"
            ),
        )
    } else {
        (ErrorKind::Syntax, reason)
    };
    BuildError {
        kind,
        message,
        span: syntax_span(pattern),
    }
}

/// Reduce an engine diagnostic to its one-line reason.
///
/// `regex`'s syntax errors render a multi-line block: a `regex parse error:`
/// header, the pattern, a caret line, then `error: <reason>`. The offending
/// span travels separately in [`BuildError`], so only the reason is kept — a
/// one-line message drops into a compiler diagnostic or a `baml.regex.Error`
/// without dragging a second copy of the pattern along.
fn condense(message: &str) -> String {
    message
        .lines()
        .rev()
        .find_map(|line| line.trim().strip_prefix("error: "))
        .unwrap_or_else(|| message.trim())
        .to_owned()
}

/// Byte span of the first problem `regex-syntax` finds in `pattern`.
///
/// `regex::Error`'s message renders a caret diagram but exposes no offsets, so
/// the pattern is re-parsed with the same front end to recover them.
fn syntax_span(pattern: &str) -> Option<(usize, usize)> {
    let err = regex_syntax::parse(pattern).err()?;
    let span = match err {
        regex_syntax::Error::Parse(e) => *e.span(),
        regex_syntax::Error::Translate(e) => *e.span(),
        _ => return None,
    };
    Some((span.start.offset, span.end.offset))
}

fn fancy_build_error(err: fancy_regex::Error) -> BuildError {
    match err {
        // `Error`'s own Display prefixes the position ("Parsing error at
        // position 3: ..."), which `BuildError.span` already carries; the inner
        // error is the reason on its own.
        fancy_regex::Error::ParseError(pos, inner) => BuildError {
            kind: ErrorKind::Syntax,
            message: condense(&inner.to_string()),
            span: Some((pos, pos.saturating_add(1))),
        },
        fancy_regex::Error::CompileError(inner) => BuildError {
            kind: ErrorKind::Syntax,
            message: condense(&inner.to_string()),
            span: None,
        },
        other => BuildError {
            kind: ErrorKind::Syntax,
            message: condense(&other.to_string()),
            span: None,
        },
    }
}

fn capture_names(engine: &Engine) -> Vec<Option<String>> {
    match engine {
        Engine::Safe(re) => re.capture_names().map(|n| n.map(str::to_owned)).collect(),
        Engine::Backtracking(re) => re.capture_names().map(|n| n.map(str::to_owned)).collect(),
    }
}

fn find_first_with(engine: &Engine, subject: &str) -> Result<Option<RawMatch>, SearchAborted> {
    match engine {
        Engine::Safe(re) => Ok(re.captures(subject).map(|caps| raw_from_safe(&caps))),
        Engine::Backtracking(re) => re
            .captures(subject)
            .map(|caps| caps.map(|caps| raw_from_fancy(&caps)))
            .map_err(|err| runtime_abort(&err)),
    }
}

fn raw_from_safe(caps: &regex::Captures<'_>) -> RawMatch {
    RawMatch {
        groups: caps
            .iter()
            .map(|g| g.map(|m| (m.start(), m.end())))
            .collect(),
    }
}

fn raw_from_fancy(caps: &fancy_regex::Captures<'_, str>) -> RawMatch {
    RawMatch {
        groups: caps
            .iter()
            .map(|g| g.map(|m| (m.start(), m.end())))
            .collect(),
    }
}

fn runtime_abort(err: &fancy_regex::Error) -> SearchAborted {
    let reason = match err {
        fancy_regex::Error::RuntimeError(fancy_regex::RuntimeError::BacktrackLimitExceeded) => {
            "backtracking limit exceeded"
        }
        fancy_regex::Error::RuntimeError(fancy_regex::RuntimeError::StackOverflow) => {
            "backtracking stack exhausted"
        }
        // Parse and compile errors cannot reach a search: the pattern already
        // compiled once. Reported the same way rather than swallowed.
        _ => "regex engine failure",
    };
    SearchAborted { reason }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_dialect_rejects_lookaround_as_unsupported() {
        let err = Program::compile("\\d+(?= USD)", false).unwrap_err();
        assert_eq!(err.kind, ErrorKind::Unsupported);
        assert!(Program::compile("\\d+(?= USD)", true).is_ok());
    }

    #[test]
    fn engine_message_is_reduced_to_one_line() {
        let err = Program::compile("(", false).unwrap_err();
        assert_eq!(err.message, "unclosed group");
        assert!(!err.message.contains('\n'));
    }

    #[test]
    fn malformed_pattern_is_syntax_in_both_dialects() {
        assert_eq!(
            Program::compile("(", false).unwrap_err().kind,
            ErrorKind::Syntax
        );
        assert_eq!(
            Program::compile("(", true).unwrap_err().kind,
            ErrorKind::Syntax
        );
    }

    #[test]
    fn unbounded_recursion_is_rejected_at_compile_time() {
        let err = Program::compile("(?<expr>\\g<expr>a|a)", true).unwrap_err();
        assert_eq!(err.kind, ErrorKind::Syntax);
        // The engine's `Error` wrapper prefix is dropped; the reason stands
        // on its own.
        assert!(
            !err.message.starts_with("Error compiling regex"),
            "{}",
            err.message
        );
    }

    #[test]
    fn word_boundaries_follow_the_literal_edges() {
        let pct = Program::word("2.5%", false).unwrap();
        assert!(pct.is_match("a 2.5% raise").unwrap());
        assert!(!pct.is_match("a 2.5%raise").unwrap());

        let cat = Program::word("cat", false).unwrap();
        assert!(!cat.is_match("concatenate").unwrap());
        assert!(cat.is_match("a cat.").unwrap());

        assert!(!Program::word("", false).unwrap().is_match("").unwrap());
        assert!(
            !Program::word("", false)
                .unwrap()
                .is_match("anything")
                .unwrap()
        );
    }

    #[test]
    fn exact_match_anchors_the_whole_alternation() {
        // `^a|b$` anchors each alternative separately, which is the trap
        // `find_exact` exists to avoid.
        let re = Program::compile("^a|b$", false).unwrap();
        assert!(re.is_match("ab").unwrap());
        assert!(re.find_exact("ab").unwrap().is_none());
        assert!(re.find_exact("a").unwrap().is_some());
    }

    #[test]
    fn exact_match_is_not_confused_by_multiline_flag() {
        let re = Program::compile("(?m)^a$", false).unwrap();
        assert!(re.is_match("x\na\ny").unwrap());
        assert!(re.find_exact("x\na\ny").unwrap().is_none());
    }

    #[test]
    fn char_offsets_handles_unsorted_multibyte_input() {
        // "😀hé" — byte offsets 0, 4, 5, 7.
        let s = "😀hé";
        assert_eq!(char_offsets(s, &[7, 0, 5, 4]), vec![3, 0, 2, 1]);
    }

    #[test]
    fn split_interleaves_participating_groups_only() {
        let re = Program::compile("(a)|(b)", false).unwrap();
        let subject = "xaybz";
        let pieces: Vec<&str> = re
            .split(subject)
            .unwrap()
            .into_iter()
            .map(|(start, end)| &subject[start..end])
            .collect();
        assert_eq!(pieces, vec!["x", "a", "y", "b", "z"]);
    }
}
