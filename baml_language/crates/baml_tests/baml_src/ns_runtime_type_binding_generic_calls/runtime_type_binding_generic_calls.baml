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
        .new("BoundItem", { "value": reflect.Type.of<string>() })
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
        .new("BoundPredicate", { "value": reflect.Type.of<string>() })
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

// The user-generic-class receiver path. `Holder<Bound>` puts a runtime-bound
// type in the receiver's own class type args, which MIR lowers through
// `inferred_ty_to_template` — a different site from the array-receiver ops
// above, and the one that reads `receiver_class_type_args`.
class Holder<T> {
    items: T[],

    function new(items: T[]) -> Holder<T> {
        Holder { items: items }
    }

    function count(self) -> int {
        self.items.length()
    }

    function matches_type(self, expected: reflect.Type) -> bool {
        reflect.Type.of<T>() == expected
    }

    function first_or_null(self) -> T? {
        self.items.at(0)
    }
}

test "method calls on a generic class receiver with a runtime-bound argument" {
    let item_t = reflect.class
        .new("BoundHeld", { "value": reflect.Type.of<string>() })
        .as_type();
    type Bound = unreflect(item_t);
    // The empty array must come from `EmptyOf<Bound>()`: a bare `[]` argument
    // literal does not unify with the runtime-bound `T[]` in the runtime gate
    // (rejected as E0001 "mismatched types"), the same class of unsolved-slot
    // answer as the KNOWN GAP on `inferred_ty_to_template`.
    let holder = Holder<Bound>.new(EmptyOf<Bound>());

    assert.equal(holder.count(), 0);
    // Pins the receiver's type argument, not just empty-list behavior: a
    // regression that widened `Bound` to `unknown` would still return 0 and
    // null above, but `reflect.Type.of<T>()` inside the method would no longer equal
    // the minted `item_t`.
    assert.is_true(holder.matches_type(item_t));
    assert.is_true(holder.first_or_null() == null)
}
