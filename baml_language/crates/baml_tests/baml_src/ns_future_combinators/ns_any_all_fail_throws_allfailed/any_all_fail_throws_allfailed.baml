// `any` throws `AllFailed` carrying every error when all futures fail.
//
// The arm binds TYPED: `AllFailed<string>` only matches because inferred
// call-site type args are threaded into `any`'s frame (`T=int, E=string`),
// so the `new AllFailed<E>(...)` constructed inside generic `any` carries
// `class_type_args = [string]` at runtime.

function bad1() -> int throws string {
    throw "a"
}

function bad2() -> int throws string {
    throw "b"
}

function main() -> int {
    let fs = [spawn { bad1() }, spawn { bad2() }];
    await baml.future.any(fs) catch (e) {
        let e: baml.future.AllFailed<string> => e.errors.length()
    }
}

test "any_all_fail_throws_allfailed" {
    assert.equal(main(), 2)
}
