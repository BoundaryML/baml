// `any` returns the first SUCCESS even when a faster future fails first.
// The fast failure is consumed by `any` (deferred-error observation), so it
// neither wins nor resurfaces fire-and-forget at `main`.

function bad() -> int throws string {
    throw "boom"
}

function good() -> int throws string {
    // The declared surface is exactly `string`, matching `bad` (Future's
    // error param is invariant, so the spawns must agree).
    baml.sys.sleep(baml.time.Duration.from_milliseconds(80n));
    42
}

function main() -> int {
    let fs = [spawn { bad() }, spawn { good() }];
    await baml.future.any(fs) catch (e) {
        let e => -1
    }
}

test "any_returns_first_success" {
    assert.equal(main(), 42)
}
