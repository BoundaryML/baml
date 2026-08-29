// A constant pattern argument to `baml.regex.compile` is compiled while the
// BAML program is, so a bad pattern is E0171 here rather than a
// `baml.regex.Error` the program has to run to reach.

// Malformed in every dialect.
function unclosed_group() -> baml.regex.Regex {
    baml.regex.compile("(unclosed")
}

// Well-formed, but outside the default dialect: the diagnostic names the
// opt-in rather than sending the author looking for a typo.
function lookahead_without_opt_in() -> baml.regex.Regex {
    baml.regex.compile("\\d+(?= USD)")
}

function backreference_without_opt_in() -> baml.regex.Regex {
    baml.regex.compile("(\\w+) \\1")
}

// BAML decodes `"\b"` as U+0008, so this pattern silently means a backspace
// character rather than a word boundary.
function backspace_instead_of_word_boundary() -> baml.regex.Regex {
    baml.regex.compile("\bword\b")
}

// A recursive subroutine that could re-enter itself without consuming input
// is rejected by the backtracking engine too.
function unbounded_recursion() -> baml.regex.Regex {
    baml.regex.compile("(?<expr>\\g<expr>a|a)", backtracking = true)
}

// ─── Accepted: no diagnostic below this line ─────────────────────────────────

function valid_default_dialect() -> baml.regex.Regex {
    baml.regex.compile("\\b\\d{4}-\\d{2}-\\d{2}\\b")
}

// The same construct rejected above, with the opt-in.
function lookahead_with_opt_in() -> baml.regex.Regex {
    baml.regex.compile("\\d+(?= USD)", backtracking = true)
}

// A pattern that is only computable at runtime is not the compiler's to
// check; that is what `baml.regex.Error` carries a message and a span for.
function dynamic_pattern(p: string) -> baml.regex.Regex {
    baml.regex.compile(p)
}

// The dialect is computed, so only a pattern *both* dialects reject would be
// reported. This one is fine under `backtracking = true`.
function computed_dialect(on: bool) -> baml.regex.Regex {
    baml.regex.compile("\\d+(?= USD)", backtracking = on)
}

// `word` escapes its literal, so its argument is text, not syntax.
function word_argument_is_never_a_pattern() -> baml.regex.Regex {
    baml.regex.word("(unclosed")
}
