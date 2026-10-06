// Ordered `all_settled` outcomes and cancellation semantics. Each native test
// keeps the old Rust harness's liveness oracle by running under a deadline.

class FasFailed {
    key: string,
}

function fas_value(n: int) -> int throws FasFailed {
    if (n < 0) {
        throw FasFailed { key: n.to_string() }
    }
    n
}

function fas_explode() -> int throws FasFailed {
    baml.sys.panic("boom");
    fas_value(2)
}

function fas_label(
    outcome: baml.future.Success<int> | baml.future.Failure<FasFailed> | baml.future.Panicked,
) -> string {
    match (outcome) {
        let success: baml.future.Success<int> => `success:${success.value}`,
        let failure: baml.future.Failure<FasFailed> => `failure:${failure.error.key}`,
        let panicked: baml.future.Panicked => match (panicked.panic) {
            baml.panics.Cancelled => "cancelled",
            baml.panics.UserPanic => "panicked",
            _ => "unexpected panic",
        },
    }
}

// A one-slot limit and the task holding that slot for `hold_ms`: anything else
// spawned with the limit starts only once the holder is done (or cancelled).
class FasGate {
    limit: baml.spawn.Limit,
    holder: baml.future.Future<null, never>,
}

function fas_gate(hold_ms: int) -> FasGate {
    let limit = baml.spawn.Limit.new(1) catch (e) {
        baml.errors.InvalidArgument => baml.sys.panic("one is positive"),
    };
    let holder = spawn with limit {
        baml.sys.sleep(baml.time.Duration.from_milliseconds(hold_ms))
    };
    FasGate { limit: limit, holder: holder }
}

function fas_failure_preserves_successes() -> string {
    let gate = fas_gate(100);
    let failed = spawn { fas_value(-1) };
    let later = spawn with gate.limit { fas_value(2) };
    defer { gate.holder.cancel() }
    let results = await baml.future.all_settled([failed, later, failed]);
    if (!later.is_settled() || later.is_cancelled()) {
        baml.sys.panic("later input must finish")
    }
    results
        .map((result) -> {
            fas_label(result)
        })
        .join(",")
}

test "failure_preserves_successes_and_waits_for_later_inputs" {
    assert.equal(
        baml.future.with_timeout(baml.time.Duration.from_milliseconds(15000), () -> {
            fas_failure_preserves_successes()
        }),
        "failure:-1,success:2,failure:-1",
    )
}

function fas_failures_in_input_order() -> string {
    let gate = fas_gate(100);
    let first = spawn with gate.limit { fas_value(-1) };
    let second = spawn { fas_value(-2) };
    defer { gate.holder.cancel() }
    let results = await baml.future.all_settled([first, second]);
    results
        .map((result) -> {
            fas_label(result)
        })
        .join(",")
}

test "every_failure_is_returned_in_input_order" {
    assert.equal(
        baml.future.with_timeout(baml.time.Duration.from_milliseconds(15000), () -> {
            fas_failures_in_input_order()
        }),
        "failure:-1,failure:-2",
    )
}

function fas_panic_and_cancellation() -> string {
    // Cancelled while still queued behind the holder: its body never runs.
    let gate = fas_gate(15000);
    defer { gate.holder.cancel() }
    let cancelled = spawn with gate.limit { fas_value(1) };
    cancelled.cancel();
    let panic = spawn { fas_explode() };
    let results = await baml.future.all_settled([panic, cancelled, spawn { fas_value(3) }]);
    match (results[0]) {
        let panicked: baml.future.Panicked => {
            let found_origin = panicked.context.stack_trace.frames.some((frame: baml.errors.StackFrame) -> bool {
                frame.file.ends_with("future_all_settled.baml")
                    && frame.function_name.includes("fas_explode")
            });
            if (!found_origin) {
                baml.sys.panic("missing original trace")
            }
            if (!(panicked.context.error is baml.panics.UserPanic)) {
                baml.sys.panic("missing original panic")
            }
            match (panicked.panic) {
                let user_panic: baml.panics.UserPanic => {
                    if (user_panic.message != "boom") {
                        baml.sys.panic("lost panic payload")
                    }
                },
                _ => baml.sys.panic("wrong panic"),
            }
        },
        _ => baml.sys.panic("missing panic outcome"),
    }
    results
        .map((result) -> {
            fas_label(result)
        })
        .join(",")
}

test "panic_and_input_cancellation_are_outcomes_with_context" {
    assert.equal(
        baml.future.with_timeout(baml.time.Duration.from_milliseconds(15000), () -> {
            fas_panic_and_cancellation()
        }),
        "panicked,cancelled,success:3",
    )
}

function fas_empty_input() -> int {
    let inputs: baml.future.Future<int, FasFailed>[] = [];
    let collected: baml.future.Future<(baml.future.Success<int> | baml.future.Failure<FasFailed> | baml.future.Panicked)[], never> = baml.future.all_settled(inputs);
    (await collected).length()
}

test "empty_input_has_no_outcomes_and_no_typed_errors" {
    assert.equal(
        baml.future.with_timeout(baml.time.Duration.from_milliseconds(15000), () -> {
            fas_empty_input()
        }),
        0,
    )
}

function fas_returned_error_is_success() -> string {
    let results = await baml.future.all_settled([spawn { FasFailed { key: "returned" } }]);
    match (results[0]) {
        let success: baml.future.Success<FasFailed> => success.value.key,
        _ => "wrong outcome",
    }
}

test "returning_an_error_value_is_still_success" {
    assert.equal(
        baml.future.with_timeout(baml.time.Duration.from_milliseconds(15000), () -> {
            fas_returned_error_is_success()
        }),
        "returned",
    )
}

function fas_cancel_collector_only() -> string {
    let gate = fas_gate(15000);
    let input = spawn with gate.limit { fas_value(7) };
    defer { gate.holder.cancel() }
    let collector = baml.future.all_settled([input]);
    baml.sys.sleep(baml.time.Duration.from_milliseconds(100));
    collector.cancel();
    let status = {
        await collector;
        "unexpected result"
    } catch (e) {
        baml.panics.Cancelled => "collector cancelled",
    };
    if (input.is_settled()) {
        baml.sys.panic("collector cancelled its input")
    }
    gate.holder.cancel();
    `${status}:${await input}`
}

test "cancelling_the_collector_stops_waiting_without_cancelling_inputs" {
    assert.equal(
        baml.future.with_timeout(baml.time.Duration.from_milliseconds(15000), () -> {
            fas_cancel_collector_only()
        }),
        "collector cancelled:7",
    )
}

// Cancelling the collector is never an input's outcome, even when the input
// settles at the same moment: here the input cancels the collector as its last
// act, so the collector wakes to its own cancellation and a settled input at
// once.
function fas_collector_cancelled_as_input_settles() -> string {
    let collectors: baml.future.Future<
        (baml.future.Success<int> | baml.future.Failure<FasFailed | baml.errors.Io> | baml.future.Panicked)[],
        never,
    >[] = [];
    let input = spawn {
        baml.sys.sleep(baml.time.Duration.from_milliseconds(20));
        collectors[0].cancel();
        fas_value(7)
    };
    collectors.push(baml.future.all_settled([input]));
    {
        await collectors[0];
        "recorded as an input outcome"
    } catch (e) {
        baml.panics.Cancelled => "collector cancelled",
    }
}

test "the_collectors_own_cancellation_is_not_an_input_outcome" {
    assert.equal(fas_collector_cancelled_as_input_settles(), "collector cancelled")
}
