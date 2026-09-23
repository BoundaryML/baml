// Regular expressions: recognizing, extracting, and rewriting structured text.
//
// A pattern is compiled once with `baml.regex.new` and the resulting
// `Regex` is an immutable, reusable value — it never remembers a previous
// match, so sharing one across calls or fibers cannot change a later result.
//
// Two engines back this namespace, selected by `new`'s `backtracking`
// argument. The default engine has a predictable upper bound on matching time
// and refuses lookaround, backreferences, and subroutine calls. The opt-in
// engine accepts them and can take exponential time on some inputs.
//
// Offsets on `Match` and `Group` are codepoint indices, so they feed
// `string.slice` directly with no byte/codepoint conversion in between.

/// One captured group of a `Match`, or the whole match itself.
///
/// `start` and `end` are codepoint offsets into the haystack, so
/// `haystack.slice(g.start, g.end) == g.text` always holds.
class Group {
    /// The text this group matched.
    text: string,
    /// Codepoint offset of the first character, inclusive.
    start: int,
    /// Codepoint offset one past the last character, exclusive.
    end: int,
}

/// One match of a pattern against a haystack.
///
/// `text`, `start`, and `end` describe the whole match. `groups[0]` repeats it
/// as a `Group`; `groups[1..]` are the numbered capture groups in the order
/// they open in the pattern.
///
/// A group that did not participate in this match — an unvisited alternation
/// branch, or a group under `?` that matched zero times — is `null` rather than
/// an empty `Group`. Named groups appear in both `groups` (by number) and
/// `named` (by name); `named` always has a key for every named group in the
/// pattern, whether or not it participated.
class Match {
    /// The whole matched text.
    text: string,
    /// Codepoint offset of the first character of the whole match, inclusive.
    start: int,
    /// Codepoint offset one past the whole match, exclusive.
    end: int,
    /// `groups[0]` is the whole match; the rest are numbered capture groups.
    /// `null` means the group did not participate.
    groups: Group?[],
    /// Named capture groups. Every name in the pattern is a key; `null` means
    /// the group did not participate.
    named: map<string, Group?>,
}

/// Why a pattern failed to compile.
enum ErrorKind {
    /// The pattern does not parse.
    Syntax,
    /// The pattern parses, but uses a construct the selected dialect does not
    /// support — lookaround, a backreference, or a subroutine call without
    /// `backtracking = true`.
    Unsupported,
    /// The pattern parses and is supported, but its compiled program exceeds
    /// the engine's size limit.
    TooLarge,
}

/// A pattern that could not be compiled.
///
/// Thrown only by `new`. Matching an existing `Regex` never throws a regex
/// error.
class Error {
    /// For portable handling: `Unsupported` is a valid pattern outside the
    /// selected dialect, `Syntax` is a malformed one.
    kind: ErrorKind,
    /// Human-readable description, including the engine's own diagnostic.
    message: string,
    /// The pattern as it was passed to `new`.
    pattern: string,
    /// Codepoint offset into `pattern` where the problem starts, when the
    /// engine reports one.
    span_start: int?,
    /// Codepoint offset one past the problem, when the engine reports one.
    span_end: int?,
}

/// A compiled regular expression.
///
/// Build one with `baml.regex.new`. Every method is a pure function of its
/// arguments: a `Regex` holds no scan position, so the same call on the same
/// haystack always returns the same result.
///
/// No method here throws: a pattern that compiled is a pattern that matches.
/// A `Regex` built with `backtracking = true` can still exhaust the engine's
/// search budget on a hostile input, which raises `baml.panics.UserPanic`
/// rather than quietly reporting "no match" — a missed match would be a wrong
/// answer, and only a `backtracking = true` pattern can reach it.
class Regex {
    /// Opaque, runtime-owned compiled program.
    _handle: $rust_type,

    /// Returns `true` if the pattern matches anywhere in `haystack`.
    ///
    /// Cheaper than `match_once` when the match itself is not needed: no
    /// capture groups are recorded.
    ///
    /// # Examples
    /// ```
    /// baml.regex.new("\\d+").has_match("a1")   // true
    /// baml.regex.new("\\d+").has_match("abc")  // false
    /// ```
    //baml:mut_vm
    //baml:fallible
    function has_match(self, haystack: string) -> bool throws never {
        $rust_function
    }

    /// Returns the leftmost match, or `null` if the pattern does not match.
    ///
    /// Among matches starting at the same position, the first alternative
    /// listed in the pattern wins.
    ///
    /// # Examples
    /// ```
    /// baml.regex.new("(\\d+)").match_once("a 42 b")?.text  // "42"
    /// baml.regex.new("\\d+").match_once("abc")             // null
    /// ```
    //baml:mut_vm
    //baml:fallible
    function match_once(self, haystack: string) -> Match? throws never {
        $rust_function
    }

    /// Returns every non-overlapping match, leftmost-first.
    ///
    /// The scan is always finite: after an empty match it advances one
    /// codepoint, so a pattern that can match the empty string yields at most
    /// one match per position rather than looping.
    ///
    /// # Examples
    /// ```
    /// baml.regex.new("\\d+").match_all("a1 b22").map((m) => m.text)  // ["1", "22"]
    /// baml.regex.new("x").match_all("abc")                           // []
    /// ```
    //baml:mut_vm
    //baml:fallible
    function match_all(self, haystack: string) -> Match[] throws never {
        $rust_function
    }

    /// Returns the match only if the pattern covers the entire haystack,
    /// `null` otherwise.
    ///
    /// This exists because hand-anchoring is subtly wrong. `^a|b$` anchors the
    /// two alternatives separately, and under `(?m)` both anchors match line
    /// boundaries rather than string boundaries.
    ///
    /// # Examples
    /// ```
    /// baml.regex.new("\\d+").match_full("42")     // matches
    /// baml.regex.new("\\d+").match_full("42 a")   // null
    /// ```
    //baml:mut_vm
    //baml:fallible
    function match_full(self, haystack: string) -> Match? throws never {
        $rust_function
    }

    /// Splits `haystack` around each match and returns the segments between
    /// them.
    ///
    /// Never returns an empty array: with no match the result is
    /// `[haystack]`. Adjacent matches, and matches at the very start or end,
    /// produce empty segments rather than being dropped.
    ///
    /// Text captured by a *participating* group is interleaved into the result
    /// after the segment that precedes it, so splitting on `(<<\d+>>)` keeps
    /// the markers. Use a non-capturing group `(?:...)` to drop them. A group
    /// that did not participate contributes nothing.
    ///
    /// # Examples
    /// ```
    /// baml.regex.new(",").split("a,b,c")       // ["a", "b", "c"]
    /// baml.regex.new("x").split("x")           // ["", ""]
    /// baml.regex.new("(,)").split("a,b")       // ["a", ",", "b"]
    /// baml.regex.new(",").split("abc")         // ["abc"]
    /// ```
    //baml:mut_vm
    //baml:fallible
    function split(self, haystack: string) -> string[] throws never {
        $rust_function
    }

    /// Replaces the first match with `template`, expanding `$0` / `$1` /
    /// `$name` / `${name}` / `$$`. Backs `string.replace`.
    //baml:mut_vm
    //baml:fallible
    function _replace_template(self, haystack: string, template: string) -> string throws never {
        $rust_function
    }

    /// Replaces every match with `template`, expanding `$0` / `$1` / `$name` /
    /// `${name}` / `$$`. Backs `string.replace_all`.
    //baml:mut_vm
    //baml:fallible
    function _replace_all_template(
        self,
        haystack: string,
        template: string,
    ) -> string throws never {
        $rust_function
    }
}

/// Compiles `pattern` into a reusable regex.
///
/// With the default `backtracking = false`, the pattern cannot use
/// lookahead, lookbehind, or backreferences and cannot catastrophically
/// backtrack. Use this mode for untrusted, user-supplied, or agent-generated
/// patterns.
///
/// Setting `backtracking = true` enables lookahead, lookbehind, and
/// backreferences, plus subroutine calls and recursion up to 20 levels. A
/// pattern compiled in this mode may take exponential time on some inputs and
/// can create a denial-of-service vulnerability. Enable it only for trusted,
/// reviewed patterns. The profiler can reveal an expensive call but does not
/// prevent or interrupt it.
///
/// Both dialects support `.`, alternation, repetition, character classes,
/// Unicode character properties, anchors, numbered and named capture groups
/// (`(?<name>...)` and Python's `(?P<name>...)`), and inline flags such as
/// `(?i)`, `(?m)`, `(?s)`, and `(?x)`. `\d`, `\w`, and `\b` are Unicode-aware
/// in both.
///
/// A constant `pattern` argument is checked while the BAML program itself is
/// compiled, so a typo is a source diagnostic rather than a runtime throw. A
/// dynamically built pattern is checked here, at construction.
///
/// # Examples
/// ```
/// baml.regex.new("\\b\\d{4}-\\d{2}-\\d{2}\\b")
/// baml.regex.new("(?i)^(yes|no)$")
/// baml.regex.new("\\b(\\w+)\\s+\\1\\b", backtracking = true)
/// ```
function new(pattern: string, backtracking: bool = false) -> Regex throws Error {
    // A `$rust_function` body has no prologue to evaluate a default in, so the
    // default lives on this wrapper and the native entry point takes both
    // arguments outright.
    _compile(pattern, backtracking)
}

/// `new` with the default already resolved.
//baml:mut_vm
function _compile(pattern: string, backtracking: bool) -> Regex throws Error {
    $rust_function
}

/// Escapes every metacharacter in `literal` so the result matches `literal`
/// itself when embedded in a pattern.
///
/// Use it whenever runtime data is interpolated into a pattern the application
/// owns.
///
/// # Examples
/// ```
/// baml.regex.escape("a.b")            // "a\\.b"
/// baml.regex.new(`(?s)<${baml.regex.escape(tag)}>(.*?)</${baml.regex.escape(tag)}>`)
/// ```
function escape(literal: string) -> string throws never {
    $rust_function
}

// ─── Replacement plumbing ────────────────────────────────────────────────────
//
// `string.replace` and `string.replace_all` differ only in a `bool`, so the
// four-way search × replacement dispatch lives in one place. It sits here
// rather than on `String` so the class that every BAML program touches does
// not carry three private helpers written against types it does not own.
//
// Each leaf is the cheapest form of the job: two literal-only native calls,
// two engine-side template expansions, and two BAML scan loops for the
// callback forms, which are the only ones that have to re-enter user code per
// match.

/// Shared body of `string.replace` (`all = false`) and `string.replace_all`.
function _replace<E>(
    haystack: string,
    search: string | Regex,
    replacement: string | ((Match) -> string throws E),
    all: bool,
) -> string throws E {
    match (search) {
        string => match (replacement) {
            string => {
                if (all) {
                    haystack._replace_all_literal(search, replacement)
                } else {
                    haystack._replace_literal(search, replacement)
                }
            },
            let f: (Match) -> string throws E => {
                _replace_literal_with(haystack, search, f, all)
            },
        },
        Regex => match (replacement) {
            string => {
                if (all) {
                    search._replace_all_template(haystack, replacement)
                } else {
                    search._replace_template(haystack, replacement)
                }
            },
            let f: (Match) -> string throws E => {
                _replace_pattern_with(haystack, search, f, all)
            },
        },
    }
}

/// Literal `search`, replacement computed per occurrence by `f`.
function _replace_literal_with<E>(
    haystack: string,
    search: string,
    f: (Match) -> string throws E,
    all: bool,
) -> string throws E {
    let length = haystack.length();
    let width = search.length();
    let pieces: string[] = [];
    let pos = 0;
    let scanning = true;
    while (scanning && pos <= length) {
        match (_index_of_from(haystack, search, pos)) {
            null => {
                scanning = false;
            },
            let at: int => {
                pieces.push(haystack.slice(pos, at));
                pieces.push(f(_literal_match(search, at)));
                // An empty `search` matches between every pair of characters.
                // Stepping one codepoint past it terminates the scan and
                // carries the stepped-over character through; the `slice` is
                // empty when the match was at the very end.
                pieces.push(
                    if (width == 0) {
                        haystack.slice(at, at + 1)
                    } else {
                        ""
                    },
                );
                pos
                    = if (width == 0) {
                        at + 1
                    } else {
                        at + width
                    };
                scanning = all;
            },
        }
    }
    pieces.push(haystack.slice(pos, length));
    pieces.join("")
}

/// Pattern `search`, replacement computed per match by `f`.
function _replace_pattern_with<E>(
    haystack: string,
    search: Regex,
    f: (Match) -> string throws E,
    all: bool,
) -> string throws E {
    let matches: Match[] = if (all) {
        search.match_all(haystack)
    } else {
        match (search.match_once(haystack)) {
            null => [],
            let first: Match => [first],
        }
    };
    let pieces: string[] = [];
    let pos = 0;
    for (let m in matches) {
        pieces.push(haystack.slice(pos, m.start));
        pieces.push(f(m));
        pos = m.end;
    }
    pieces.push(haystack.slice(pos, haystack.length()));
    pieces.join("")
}

/// Builds the `Match` that a literal (non-pattern) search reports.
///
/// A literal search has no capture groups, so `groups` holds only the whole
/// match and `named` is empty. Used by `string.replace` / `string.replace_all`
/// when `search` is a `string` and `replacement` is a callback.
function _literal_match(text: string, start: int) -> Match throws never {
    let end = start + text.length();
    Match {
        text: text,
        start: start,
        end: end,
        groups: [Group { text: text, start: start, end: end }],
        named: {},
    }
}

/// Internal offset search for callback replacement; returns an absolute
/// codepoint index without constructing the remaining suffix. An offset
/// outside `[0, length]` returns null, including for an empty search.
function _index_of_from(haystack: string, search: string, start: int) -> int? throws never {
    $rust_function
}
