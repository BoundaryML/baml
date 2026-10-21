// docs:start all
// docs:start estimate
function estimate_tokens(text: string, chars_per_token: int = 4) -> int {
    text.trim().length() / chars_per_token
}

// docs:end estimate

// docs:start cost
function cost_cents(tokens: int, price_cents_per_mtok: int) -> int {
    tokens * price_cents_per_mtok / 1_000_000
}

// docs:end cost

// docs:start main
function main() -> string {
    let transcript = "user: Where is my order?\nagent: It shipped on Tuesday.";
    let prose = estimate_tokens(transcript);
    let dense = estimate_tokens(transcript, chars_per_token = 3);
    `between ${prose} and ${dense} tokens`
}
// docs:end main
// docs:end all

test "chapter example" {
    assert.equal(main(), "between 13 and 18 tokens")
}

test "token estimate defaults and named arguments" {
    assert.equal(estimate_tokens(""), 0);
    assert.equal(estimate_tokens("    "), 0);
    assert.equal(estimate_tokens("abcdefgh"), 2);
    assert.equal(estimate_tokens(chars_per_token = 3, text = "abcdefghi"), 3)
}
