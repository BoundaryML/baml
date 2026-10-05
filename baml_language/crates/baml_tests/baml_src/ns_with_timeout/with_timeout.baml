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

// A body that is a function VALUE, not a lambda written at the call. The call
// throws what the type of the value says, plus the timeout: a `catch` must see
// both facts, and a `catch_all` binding must be a value of them.
class WithTimeoutBodyError {
    code: int,
}

function with_timeout_body_fails() -> string throws WithTimeoutBodyError {
    throw WithTimeoutBodyError { code: 7 }
}

function with_timeout_body_sleeps() -> string throws baml.errors.Io {
    baml.sys.sleep(baml.time.Duration.from_seconds(60));
    "finished"
}

function with_timeout_of_function_value(body: () -> string throws WithTimeoutBodyError) -> string {
    baml.future.with_timeout(baml.time.Duration.from_seconds(30), body) catch (e) {
        baml.errors.Timeout => "timed out",
        WithTimeoutBodyError => `body error ${e.code}`,
    }
}

test "with_timeout of a function value: catch sees the error of the value" {
    assert.equal(with_timeout_of_function_value(with_timeout_body_fails), "body error 7")
}

function with_timeout_catch_all_of_function_value(body: () -> string throws unknown) -> string {
    baml.future.with_timeout(baml.time.Duration.from_milliseconds(25), body) catch_all (e) {
        _ => e.to_string(),
    }
}

test "with_timeout of a function value: a catch_all binding is the thrown value" {
    assert.equal(
        with_timeout_catch_all_of_function_value(with_timeout_body_fails),
        "WithTimeoutBodyError { code: 7 }",
    );
    assert.equal(
        with_timeout_catch_all_of_function_value(with_timeout_body_sleeps),
        "Timeout { message: \"operation timed out after 25ms\", duration: Duration { _nanoseconds: 25000000 }, timeout_type: \"limit\" }",
    )
}

// The same through a generic caller, as a wrapper of `with_timeout` is written.
class WithTimeoutSettled {
    detail: string,
}

function with_timeout_settle<T>(body: () -> T throws unknown) -> T | WithTimeoutSettled {
    baml.future.with_timeout(baml.time.Duration.from_seconds(30), body) catch_all (e) {
        _ => WithTimeoutSettled { detail: e.to_string() },
    }
}

test "with_timeout of a function value in a generic wrapper" {
    let settled = with_timeout_settle(with_timeout_body_fails);
    match (settled) {
        let failure: WithTimeoutSettled => {
            assert.equal(failure.detail, "WithTimeoutBodyError { code: 7 }")
        },
        let value: string => assert.is_true(false),
    }
}
