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

// A caller cancelled as its deadline fires reports its own cancellation, not a
// timeout: the body's cleanup cancels the caller while the deadline cancels
// the body, so the caller wakes to both at once. The deadline leaves the body
// time to start: a body cancelled before it starts never runs its cleanup.
function with_timeout_caller_cancelled_at_deadline() -> string {
    let callers: baml.future.Future<string, baml.errors.Io>[] = [];
    callers.push(spawn {
        baml.future.with_timeout(baml.time.Duration.from_milliseconds(250), () -> string {
            defer {
                callers[0].cancel();
            }
            baml.sys.sleep(baml.time.Duration.from_seconds(30));
            "finished"
        }) catch (e) {
            baml.errors.Timeout => "timed out",
        }
    });
    (await callers[0]) catch (e) {
        baml.panics.Cancelled => "caller cancelled",
    }
}

test "with_timeout reports its caller's cancellation over its deadline" {
    assert.equal(with_timeout_caller_cancelled_at_deadline(), "caller cancelled")
}
