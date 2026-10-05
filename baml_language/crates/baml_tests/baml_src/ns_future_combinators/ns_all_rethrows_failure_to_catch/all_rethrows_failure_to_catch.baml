// `all` re-throws the first failure; the surrounding `catch` observes it.
// The failing input's error is consumed by `all`'s await (deferred-error
// observation, see `tests/fire_and_forget.rs`), so it does NOT also resurface
// fire-and-forget at `main`.

function ok() -> int throws string {
    1
}

function bad() -> int throws string {
    throw "boom"
}

function main() -> int {
    // Future's error param is invariant, so both spawns must produce the
    // SAME error type; `ok` declares (without using) the same `throws
    // string` as `bad` to keep the array element type homogeneous.
    let fs = [spawn { ok() }, spawn { bad() }];
    let r = await baml.future.all(fs) catch (e) {
        let e => [99]
    };
    r[0]
}

test "all_rethrows_failure_to_catch" {
    assert.equal(main(), 99)
}
