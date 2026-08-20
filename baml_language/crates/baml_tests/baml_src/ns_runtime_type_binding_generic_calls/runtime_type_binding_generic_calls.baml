// Characterization net for generic calls made in the scope of a
// `type T = unreflect(expr)` binding.
//
// Such a call is deferred to a runtime gate rather than solved statically, so
// inference finalizes its own slots to an error-recovery sentinel (a still-free
// var erases to `Error`). MIR lowering used to hand that sentinel straight to
// runtime lowering, which treats one as a compiler bug — so this well-typed
// source ICEd the compiler instead of compiling at all.

function EmptyOf<T>() -> T[] {
    []
}

test "non-closure generic list ops over a runtime-bound element type" {
    let item_t = reflect.class
        .new("BoundItem", { "value": type.of<string>() })
        .as_type();
    type Bound = unreflect(item_t);
    let items = EmptyOf<Bound>();

    assert.equal(items.length(), 0);
    assert.is_true(items.at(0) == null);
    assert.equal(items.slice(0, 0).length(), 0)
}

// The shape that ICEd, guarded at COMPILE time only — the crash was in MIR
// lowering, which every corpus file goes through, and this function is
// deliberately never called.
//
// It is not executed because the runtime gate still rejects it: the unsolved
// slot MIR seeds is `baml.Array.filter`'s own `E`, an effect parameter whose
// language default is `never` rather than the top type MIR widens to.
// `default_unsolved_effects_to_never` only recognizes synthetic
// `__effect_param_N` params, so a user-written `filter<E>(…) throws E` is
// missed. Promote this to a `test` once that is fixed.
function ClosureOverRuntimeBoundElement() -> int {
    let item_t = reflect.class
        .new("BoundPredicate", { "value": type.of<string>() })
        .as_type();
    type Bound = unreflect(item_t);
    let items = EmptyOf<Bound>();

    items
        .filter((item) -> {
            let _ = item;
            true
        })
        .length()
}
