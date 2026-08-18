// Tests for `baml.future.with_timeout` — the deadline combinator that bounds
// any blocking body, since `baml.io.Read.read` and friends take no timeout of
// their own.
//
// Both cases are deterministic: the sleeps differ by orders of magnitude, so
// neither outcome depends on scheduling luck.

// A body that finishes well inside the limit yields its value.
function with_timeout_returns_the_value_fn() -> int {
    baml.future.with_timeout(
        baml.time.Duration.from_seconds(30),
        () -> {
            42
        },
    )
}

test "with_timeout_returns_the_value" {
    assert.equal(with_timeout_returns_the_value_fn(), 42)
}

// A body still parked when the limit elapses is cancelled and reported as
// `baml.errors.Timeout`; the sleeping body would otherwise run for a minute.
function with_timeout_throws_on_expiry_fn() -> string {
    baml.future.with_timeout(
        baml.time.Duration.from_milliseconds(25),
        () -> {
            baml.sys.sleep(baml.time.Duration.from_seconds(60));
            "finished"
        },
    ) catch (e) {
        baml.errors.Timeout => "timed out",
    }
}

test "with_timeout_throws_on_expiry" {
    assert.equal(with_timeout_throws_on_expiry_fn(), "timed out")
}

// The body's own error is not a timeout: it propagates with its own type.
function with_timeout_propagates_body_errors_fn() -> string {
    baml.future.with_timeout(
        baml.time.Duration.from_seconds(30),
        () -> {
            throw baml.errors.Io { message: "from the body" }
        },
    ) catch (e) {
        baml.errors.Timeout => "timed out",
        baml.errors.Io => "io",
    }
}

test "with_timeout_propagates_body_errors" {
    assert.equal(with_timeout_propagates_body_errors_fn(), "io")
}
