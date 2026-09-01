function with_timeout_returns_value() -> int {
    baml.future.with_timeout(baml.time.Duration.from_seconds(30), () -> {
        42
    })
}

test "with_timeout returns value" {
    assert.equal(with_timeout_returns_value(), 42)
}

function with_timeout_expires() -> string {
    baml.future.with_timeout(baml.time.Duration.from_milliseconds(25), () -> {
        baml.sys.sleep(baml.time.Duration.from_seconds(60));
        "finished"
    }) catch (e) {
        baml.errors.Timeout => "timed out",
    }
}

test "with_timeout expires" {
    assert.equal(with_timeout_expires(), "timed out")
}

function with_timeout_preserves_body_error() -> string {
    baml.future.with_timeout(baml.time.Duration.from_seconds(30), () -> {
        throw baml.errors.Io { message: "body error" }
    }) catch (e) {
        baml.errors.Timeout => "timed out",
        baml.errors.Io => "io",
    }
}

test "with_timeout preserves body error" {
    assert.equal(with_timeout_preserves_body_error(), "io")
}
