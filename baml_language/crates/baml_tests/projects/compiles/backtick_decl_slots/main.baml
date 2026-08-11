// BEP-049 backtick string literals in the declarative slots that the formatter
// used to reject (it only knew the quoted and raw string tokens there). Both
// slots below compile cleanly and format without error.
//
// `baml fmt` re-indents a multi-line backtick interior to sit one level past the
// surrounding block (like a raw string), but only when that preserves the
// string's auto-dedented (BEP-049 §12) runtime value. This file is already in
// canonical form, so `baml fmt` leaves it unchanged.

client<llm> Fast {
    provider: openai,
    options: {
        model: "gpt-4o-mini",
        api_key: env.OPENAI_API_KEY,
    },
}

// Backtick value in `prompt:` position (the originally reported crash).
function Greet(name: string) -> string {
    client: Fast
    prompt: `
        Say hello to ${name}.
    `
}

// Backtick value returned by a regular function.
function Header(title: string) -> string {
    `
        # ${title}
    `
}
