// `AllFailed.errors` is in INPUT order even when failures settle out of
// order: the first input fails slowly, the second fast — the aggregate must
// still read `["a", "b"]`.

function bad_slow() -> int throws string {
    // sleep's own `throws baml.errors.Io` is swallowed so the declared
    // surface stays exactly `string`.
    baml.sys.sleep(baml.time.Duration.from_milliseconds(120n)) catch (e) {
        let e => null
    };
    throw "a"
}

function bad_fast() -> int throws string {
    throw "b"
}

function main() -> int {
    let fs = [spawn { bad_slow() }, spawn { bad_fast() }];
    await baml.future.any(fs) catch (e) {
        let e: baml.future.AllFailed<string> => {
            if (e.errors.length() == 2 && e.errors[0] == "a" && e.errors[1] == "b") {
                1
            } else {
                0
            }
        }
    }
}

test "any_all_fail_errors_in_input_order" {
    assert.equal(main(), 1)
}
