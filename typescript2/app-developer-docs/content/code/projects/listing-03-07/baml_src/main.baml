// docs:start all
// docs:start estimate
/// Estimates token usage from character count.
/// This is a rough estimate, not a model-specific tokenizer.
function estimate_tokens(text: string, chars_per_token: int = 4) -> int {
    //# Count the characters
    let chars = text.trim().length();
    //# Divide by the average token width
    chars / chars_per_token
}

// docs:end estimate
function main() -> int {
    estimate_tokens("How many tokens will this message use?")
}
// docs:end all

test "chapter example" {
    assert.equal(main(), 9)
}

test "token estimate defaults and named arguments" {
    assert.equal(estimate_tokens(""), 0);
    assert.equal(estimate_tokens("    "), 0);
    assert.equal(estimate_tokens("abcdefgh"), 2);
    assert.equal(estimate_tokens(chars_per_token = 3, text = "abcdefghi"), 3)
}
