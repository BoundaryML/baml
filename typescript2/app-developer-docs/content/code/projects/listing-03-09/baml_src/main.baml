// docs:start all
function estimate_tokens(text: string, chars_per_token: int = 4) -> int {
    text.trim().length() / chars_per_token
}

function total_tokens(messages: string[]) -> int {
    let total = 0;
    // docs:start loop
    for (let message in messages) {
        if (message.trim() == "") {
            continue;
        }
        total = total + estimate_tokens(message);
    }
    // docs:end loop
    total
}

function main() -> int {
    total_tokens(["Order status?", "", "It shipped Tuesday."])
}
// docs:end all

test "chapter example" {
    assert.equal(main(), 7)
}

test "token estimate defaults and named arguments" {
    assert.equal(estimate_tokens(""), 0);
    assert.equal(estimate_tokens("    "), 0);
    assert.equal(estimate_tokens("abcdefgh"), 2);
    assert.equal(estimate_tokens(chars_per_token = 3, text = "abcdefghi"), 3)
}

test "blank messages and empty conversation" {
    assert.equal(total_tokens([]), 0);
    assert.equal(total_tokens(["", "   "]), 0);
    assert.equal(total_tokens(["abcd", "", "abcdefgh"]), 3)
}
