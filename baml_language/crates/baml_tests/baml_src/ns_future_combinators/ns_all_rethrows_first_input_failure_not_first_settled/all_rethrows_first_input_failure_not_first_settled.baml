// `all` observes failures in INPUT order: the first input failing slowly
// must still be the error `all` rethrows, even though the second failed
// first in wall-clock time.

function bad_slow() -> int throws string {
    baml.sys.sleep(baml.time.Duration.from_milliseconds(120n));
    throw "first"
}

function bad_fast() -> int throws string {
    throw "second"
}

function helper(fs: baml.future.Future<int, string>[]) -> string throws string {
    let r = await baml.future.all(fs);
    "no-throw"
}

function main() -> string {
    let fs = [spawn { bad_slow() }, spawn { bad_fast() }];
    helper(fs) catch (e) {
        let s: string => s
    }
}

test "all_rethrows_first_input_failure_not_first_settled" {
    assert.equal(main(), "first")
}
