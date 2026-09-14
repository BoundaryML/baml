// Generic calls made in the scope of a `type T = unreflect(expr)` binding.
// `Bound` is a rigid parameter of the frame, so every callee slot it fills
// solves statically and lowers as an ordinary type argument.

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

// The shape that ICEd. Inferring the stored lambda parameter also lets the
// callback's `never` effect reach `Array.filter`'s `E` slot.
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

test "closure generic list ops over a runtime-bound element type" {
    assert.equal(ClosureOverRuntimeBoundElement(), 0)
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
    // A bare `[]` argument solves its element slot from the declared `T[]`
    // with `T = Bound`, exactly as it would for a static type argument.
    let holder = Holder<Bound>.new([]);

    assert.equal(holder.count(), 0);
    // Pins the receiver's type argument, not just empty-list behavior: a
    // regression that widened `Bound` to `unknown` would still return 0 and
    // null above, but `reflect.Type.of<T>()` inside the method would no longer equal
    // the minted `item_t`.
    assert.is_true(holder.matches_type(item_t));
    assert.is_true(holder.first_or_null() == null)
}
