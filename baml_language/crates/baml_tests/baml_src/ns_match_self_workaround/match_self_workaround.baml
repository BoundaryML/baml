// The sound workaround for `Self` in a class-method body: name the enclosing
// type explicitly. `Self` is a signature-only construct for now — a
// body-position `Self` is a compile error (pinned in the `self_in_body`
// diagnostics project). In a class method the equivalent is to write the type
// out.
//
// This pins the *working side* of that restriction: for a NON-generic class,
// naming the enclosing type in a body `match`/`is` routes through the ordinary
// class-identity test and works. These stay green as `Self` support expands.
//
// BUG(class-typevar-body-match): the analogous GENERIC-class workaround —
// writing `Box<T>` (the class's own type param `T`) in a method-body `match`/`is`
// — does NOT yet work: the runtime dispatch-guard for a class-level type
// parameter fails to match even the receiver's own instantiation, so a
// `Box<int>` value on a `Box<int>` receiver takes the fallthrough arm. This is a
// pre-existing gap (Unit-2 / B-634 typevar matching was only ever exercised with
// *function* generics; `reflect.type_of<T>()` reads the frame's `T` fine, but
// the guard match does not), independent of the `Self` restriction. It is the
// same "expand `Self` support" bucket: resolving a generic class's `Self`/`T` in
// a body requires this runtime path to work first. Generic tests belong here
// once it does.

class Shape {
    sides: int,
    function same_kind(self, other: Shape | int) -> string {
        match (other) {
            let s: Shape => "shape",
            _ => "other",
        }
    }
    function is_shape(self, other: Shape | int) -> bool {
        other is Shape
    }
}

function shape_matches_shape_fn() -> string {
    let s = Shape { sides: 3 };
    s.same_kind(Shape { sides: 4 })
}

test "self_workaround_match_captures_enclosing_type" {
    assert.equal(shape_matches_shape_fn(), "shape")
}

function shape_matches_int_fn() -> string {
    let s = Shape { sides: 3 };
    s.same_kind(42)
}

test "self_workaround_match_rejects_other_type" {
    assert.equal(shape_matches_int_fn(), "other")
}

function shape_is_shape_fn() -> bool {
    let s = Shape { sides: 3 };
    s.is_shape(Shape { sides: 4 })
}

test "self_workaround_is_enclosing_type" {
    assert.is_true(shape_is_shape_fn())
}

function shape_is_not_int_fn() -> bool {
    let s = Shape { sides: 3 };
    s.is_shape(7)
}

test "self_workaround_is_rejects_other_type" {
    assert.is_true(!shape_is_not_int_fn())
}
