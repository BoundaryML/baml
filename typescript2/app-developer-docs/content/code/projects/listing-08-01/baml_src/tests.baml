// These cases take deterministic branches before invoke_tool reaches int.random.
test "selected handling returns only its matching error" {
    assert.equal(check_tool_name("search"), "search");
    assert.equal(display_tool_name(""), "tool name is empty");
    assert.equal(handle_bad_input(""), "invalid tool input: tool name is empty");
}

test "exhaustive handlers and wildcards return their declared fallbacks" {
    assert.equal(run_step(""), "invalid tool input: tool name is empty");
    assert.equal(run_step_with_fallback(""), "invalid tool input: tool name is empty");
    assert.equal(catch_every_error(), "caught error");
    assert.equal(run_step_with_logging(""), "tool failed");
}

test "an unhandled error propagates through the checked wrapper" {
    let result = invoke_tool_checked("") catch (e) {
        BadToolInput => `propagated: ${e.message}`,
    };
    assert.equal(result, "propagated: tool name is empty");
}

test "panics cross catch_all and stop at a panic boundary" {
    let escaped = panic_escapes_catch_all("search") catch_all_panics (e) {
        _ => "panic reached outer boundary",
    };
    assert.equal(escaped, "panic reached outer boundary");
    assert.equal(panic_escapes_catch_all(""), "caught error");
    assert.equal(handle_web_request("search"), 500);
    assert.equal(handle_web_request(""), 500);
}

test "creating a lambda has no effect and calling it propagates its effect" {
    assert.equal(define_validator(), "validator ready");
    assert.equal(validate_score(7), 7);
    let failed = validate_score(-1) catch (e) {
        BadToolInput => e.message,
    };
    assert.equal(failed, "score is negative");
    assert.equal(apply_validator((score: int) -> int { score + 1 }, 4), 5);
}
