// A typed catch arm naming the ENCLOSING function's `TypeVar`
// (`AllFailed<E>` inside `helper<E>`) matches at runtime: the pattern
// lowers to a `TypeArgRef` template resolved against helper's frame
// (`E = string`, inferred at main's call site), and the instance thrown by
// generic `any` carries `class_type_args = [string]` through the
// main → helper → any inferred generic chain.

function bad() -> int throws string {
    throw "x"
}

function helper<E>(fs: baml.future.Future<int, E>[]) -> int {
    (await baml.future.any(fs)) catch (e) {
        let e: baml.future.AllFailed<E> => e.errors.length()
    }
}

function main() -> int {
    helper([spawn { bad() }, spawn { bad() }])
}

test "typed_arm_on_enclosing_typevar_matches" {
    assert.equal(main(), 2)
}
