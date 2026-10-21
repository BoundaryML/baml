// docs:start all
// docs:start void
function report_budget(tokens: int, window: int) -> void {
    if (tokens <= window) {
        return;
    }
    log.info(`over budget by ${tokens - window} tokens`);
}

// docs:end void

// docs:start early
function clamp_tokens(tokens: int, window: int) -> int {
    if (tokens > window) {
        return window;
    }
    tokens
}

// docs:end early

// docs:start main
function main() -> int {
    report_budget(9_000, 8_000);
    clamp_tokens(9_000, 8_000)
}
// docs:end main
// docs:end all

test "early return and final expression" {
    assert.equal(clamp_tokens(9000, 8000), 8000);
    assert.equal(clamp_tokens(8000, 8000), 8000);
    assert.equal(clamp_tokens(500, 8000), 500);
    report_budget(500, 8000);
    assert.equal(main(), 8000)
}
