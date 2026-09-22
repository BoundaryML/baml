// docs:start all
function count_chunks(tokens: int, window: int) -> int {
    let remaining = tokens;

    let chunks = 0;
    while (remaining > 0) {
        remaining = remaining - window;
        chunks = chunks + 1;
    }
    chunks
}
// docs:end all

function main() -> int { count_chunks(26_000, 8_000) }

test "chapter example" {
    assert.equal(main(), 4)
}

test "chunk boundaries" {
    assert.equal(count_chunks(0, 8000), 0);
    assert.equal(count_chunks(8000, 8000), 1);
    assert.equal(count_chunks(8001, 8000), 2);
    assert.equal(count_chunks(16000, 8000), 2)
}
