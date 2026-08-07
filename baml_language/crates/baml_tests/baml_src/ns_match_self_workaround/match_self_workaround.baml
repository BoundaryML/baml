// The sound workaround for `Self` in a PLAIN class-method body: name the
// enclosing type explicitly. A plain class body cannot yet name its own
// instantiation as `Self` — body-position `Self` there is an unresolved-name
// error (pinned in the `self_in_body` diagnostics project) — so the
// equivalent is to write the type out. (`Self` in interface default methods
// and in implements-block bodies IS supported — the rigid frame-slot variable
// and the statically substituted `for` target respectively; pinned in
// `ns_self_frame_slot`.)
//
// This pins the workaround for both shapes: a NON-generic class names itself
// (a class-identity test), and a GENERIC class names itself at its own type
// param (`BoxG<T>` — an arg-precise invariant test against the receiver's
// frame). These stay green as `Self` support expands to plain class bodies.

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


// ── The generic-class workaround: naming the class at its own param ─────────

class BoxG<T> {
    value: T,
    function same_box(self, other: BoxG<T> | int) -> string {
        match (other) {
            let b: BoxG<T> => "box",
            _ => "other",
        }
    }
    function is_own_instantiation(self, other: unknown) -> bool {
        other is BoxG<T>
    }
}

function box_matches_own_instantiation_fn() -> string {
    let b: BoxG<int> = BoxG { value: 3 };
    b.same_box(BoxG { value: 4 })
}

test "self_workaround_generic_match_captures_own_instantiation" {
    assert.equal(box_matches_own_instantiation_fn(), "box")
}

function box_matches_int_fn() -> string {
    let b: BoxG<int> = BoxG { value: 3 };
    b.same_box(42)
}

test "self_workaround_generic_match_rejects_other_type" {
    assert.equal(box_matches_int_fn(), "other")
}

function box_is_own_instantiation_fn() -> bool {
    let b: BoxG<int> = BoxG { value: 3 };
    b.is_own_instantiation(BoxG { value: 4 })
}

test "self_workaround_generic_is_own_instantiation" {
    assert.is_true(box_is_own_instantiation_fn())
}

function box_is_other_instantiation_fn() -> bool {
    let b: BoxG<int> = BoxG { value: 3 };
    b.is_own_instantiation(BoxG { value: "s" })
}

test "self_workaround_generic_is_rejects_other_instantiation" {
    assert.is_true(!box_is_other_instantiation_fn())
}

function box_is_not_int_fn() -> bool {
    let b: BoxG<int> = BoxG { value: 3 };
    b.is_own_instantiation(7)
}

test "self_workaround_generic_is_rejects_other_type" {
    assert.is_true(!box_is_not_int_fn())
}
