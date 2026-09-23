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
    /// Anchored twin used by [`Program::find_exact`], checked at construction
    /// so matching cannot discover a second compilation error.
    ///
    /// Anchoring has to happen in the pattern rather than by checking a match's
    /// span: leftmost-first semantics can pick a shorter match at position 0
    /// while a longer one covering the whole subject exists. `\A` / `\z` (not
    /// `^` / `$`) so `(?m)` cannot reinterpret them as line boundaries.
    anchored: Engine,
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
        if backtracking && let Some(start) = whole_pattern_recursion(pattern) {
            return Err(BuildError {
                kind: ErrorKind::Unsupported,
                message: "whole-pattern recursion (`\\g<0>`) is not supported".to_owned(),
                span: Some((start, start + r"\g<0>".len())),
            });
        }
        // Enable extended mode before the newline: it is ignored whether or
        // not the original pattern enabled `x`. If the pattern ends in an
        // extended-mode comment, the newline ends that comment before our
        // closing group and anchor. The noncapturing wrapper preserves group
        // numbers and contains any flags changed by the original pattern.
        let wrapped = format!("\\A(?:{pattern}(?x)\n)\\z");
        let anchored = build_engine(&wrapped, backtracking).map_err(|mut err| {
            // A wrapper failure (for example the compiled size limit) is not
            // located in the user's pattern; do not expose wrapper offsets.
            err.span = None;
            err
        })?;
        let names = capture_names(&engine);
        Ok(Program {
            engine,
            anchored,
            names,
            pattern: pattern.to_owned(),
            backtracking,
        })
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
        Ok(
            find_first_with(&self.anchored, subject)?
                .filter(|raw| raw.span() == (0, subject.len())),
        )
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

/// Find an unescaped whole-pattern recursion token.
///
/// `match_full` compiles an anchored twin of every pattern. A `\g<0>` call
/// changes meaning inside that wrapper, so reject it instead of letting the
/// search and full-match forms disagree. Escaped backslashes remain literals.
fn whole_pattern_recursion(pattern: &str) -> Option<usize> {
    let bytes = pattern.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'\\' {
            i += 1;
            continue;
        }
        if bytes.get(i + 1) == Some(&b'\\') {
            i += 2;
            continue;
        }
        if bytes[i..].starts_with(br"\g<0>") {
            return Some(i);
        }
        i += 1;
    }
    None
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

/// Compile the selected dialect and normalize its construction errors.
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

/// Classify backtracking-engine failures, preserving source byte spans when available.
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
        fancy_regex::Error::CompileError(inner) => {
            let kind = match inner.as_ref() {
                fancy_regex::CompileError::InnerError(err) if err.size_limit().is_some() => {
                    ErrorKind::TooLarge
                }
                fancy_regex::CompileError::FeatureNotYetSupported(_)
                | fancy_regex::CompileError::VariableLookBehindRequiresFeature => {
                    ErrorKind::Unsupported
                }
                _ => ErrorKind::Syntax,
            };
            BuildError {
                kind,
                message: condense(&inner.to_string()),
                span: None,
            }
        }
        other => BuildError {
            kind: ErrorKind::Syntax,
            message: condense(&other.to_string()),
            span: None,
        },
    }
}

/// Return capture names by group number, including the unnamed whole-match slot.
fn capture_names(engine: &Engine) -> Vec<Option<String>> {
    match engine {
        Engine::Safe(re) => re.capture_names().map(|n| n.map(str::to_owned)).collect(),
        Engine::Backtracking(re) => re.capture_names().map(|n| n.map(str::to_owned)).collect(),
    }
}

/// Find captures with either engine without swallowing a backtracking abort.
fn find_first_with(engine: &Engine, subject: &str) -> Result<Option<RawMatch>, SearchAborted> {
    match engine {
        Engine::Safe(re) => Ok(re.captures(subject).map(|caps| raw_from_safe(&caps))),
        Engine::Backtracking(re) => re
            .captures(subject)
            .map(|caps| caps.map(|caps| raw_from_fancy(&caps)))
            .map_err(|err| runtime_abort(&err)),
    }
}

/// Copy safe-engine capture byte spans, retaining nonparticipating groups as `None`.
fn raw_from_safe(caps: &regex::Captures<'_>) -> RawMatch {
    RawMatch {
        groups: caps
            .iter()
            .map(|g| g.map(|m| (m.start(), m.end())))
            .collect(),
    }
}

/// Copy backtracking capture byte spans using the same layout as the safe engine.
fn raw_from_fancy(caps: &fancy_regex::Captures<'_, str>) -> RawMatch {
    RawMatch {
        groups: caps
            .iter()
            .map(|g| g.map(|m| (m.start(), m.end())))
            .collect(),
    }
}

/// Convert engine exhaustion into a stable reason for the VM panic boundary.
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
    fn whole_pattern_recursion_is_rejected_before_anchoring() {
        let err = Program::compile(r"a\g<0>?", true).unwrap_err();
        assert_eq!(err.kind, ErrorKind::Unsupported);
        assert_eq!(err.span, Some((1, 6)));

        let escaped = Program::compile(r"\\g<0>", true).unwrap();
        assert!(escaped.find_exact(r"\g<0>").unwrap().is_some());

        let malformed = Program::compile(r"(a\g<0>?", true).unwrap_err();
        assert_eq!(malformed.kind, ErrorKind::Syntax);
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
    fn exact_match_preserves_comments_flags_and_captures() {
        for backtracking in [false, true] {
            for pattern in ["(?x)(?<letter>a) # trailing comment", "(?<letter>a)"] {
                let re = Program::compile(pattern, backtracking).unwrap();
                let found = re.find_exact("a").unwrap().unwrap();
                assert_eq!(found.groups, vec![Some((0, 1)), Some((0, 1))]);
                assert_eq!(re.names()[1].as_deref(), Some("letter"));
                assert!(re.find_exact("a\n").unwrap().is_none());
            }
            let re = Program::compile("(?x)a(?-x) ", backtracking).unwrap();
            assert!(re.find_exact("a ").unwrap().is_some());
            assert!(re.find_exact("a").unwrap().is_none());
            let re = Program::compile("a|ab", backtracking).unwrap();
            assert_eq!(re.find_exact("ab").unwrap().unwrap().span(), (0, 2));
        }
    }

    #[test]
    fn oversized_pattern_has_the_same_error_kind_in_both_dialects() {
        for backtracking in [false, true] {
            let err = Program::compile(r"\w{10000}", backtracking).unwrap_err();
            assert_eq!(err.kind, ErrorKind::TooLarge, "{err:?}");
        }
    }

    #[test]
    fn exact_match_requires_the_reported_span_to_cover_the_subject() {
        let re = Program::compile(r"a\Kb", true).unwrap();
        assert_eq!(re.find_first("ab").unwrap().unwrap().span(), (1, 2));
        assert!(re.find_exact("ab").unwrap().is_none());
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
