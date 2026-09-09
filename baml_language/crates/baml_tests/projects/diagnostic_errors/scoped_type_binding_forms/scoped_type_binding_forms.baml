// The two contracts a `type T = …` statement has beyond scoping: a static
// right-hand side is written in full, and a runtime operand is a
// `reflect.Type | reflect.TypeView` value.

class Wrapper<T> { inner T }

function ident<T>(v: T) -> T throws never { v }

// A `_` in the right-hand side has nothing to infer from: the slot is filled
// from the written type when the statement runs.
function hole_in_a_static_binding() -> int {
    type T = Wrapper<_>
    1
}

// An `unknown` is not a type operand. A value the checker has not typed may
// not be erased into the frame to fail at run time: narrow it first with
// `match` or `is` (pinned in `ns_scoped_type_bindings`).
function unknown_operand(x: unknown) -> bool {
    type T = unreflect(x)
    reflect.Type.of<T>() == reflect.Type.of<int>()
}

// An open operand is judged when its variable solves. BUG: it never does
// here. The variable carries the literal `1` below and the operand contract
// above, and a class whose bounds conflict is dropped without a report (the
// same road that lets `ys: string[]` take an inferred `int`), so this
// compiles clean and reaches the VM as an internal error. Expected: E0001
// at `v`.
function operand_solved_to_int() -> int {
    let v = ident(1)
    type T = unreflect(v)
    1
}

// An attribute has no field to attach to in a binding.
function attribute_on_the_right_hand_side(t: reflect.Type) -> int {
    type T = unreflect(t) @alias("x")
    1
}

// `unreflect` inside a static right-hand side: the statement lifts one
// runtime type as its whole right-hand side, so compose through a second
// binding. Parentheses open a function type, so they nest it too.
function nested_in_a_static_right_hand_side(t: reflect.Type) -> int {
    type T = Wrapper<unreflect(t)>
    type U = (unreflect(t))
    1
}
