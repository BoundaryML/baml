function step(value: int) -> int {
    value + 1
}

function cpu(seed: int) -> int {
    let value = seed;
    for (let i = 0; i < 1000; i += 1) {
        value = step(value);
    }
    value
}

function capture(seed: int, payload: string) -> int {
    seed
}

function many_captures() -> int {
    let total = 0;
    for (let i = 0; i < 256; i += 1) {
        total += capture(i, "payload");
    }
    total
}

test "telemetry performance results" {
    assert.equal(cpu(5), 1005);
    assert.equal(many_captures(), 32640);
    assert.equal(capture(9, "payload"), 9)
}
