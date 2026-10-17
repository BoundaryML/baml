// The contracts a `let type` statement has beyond scoping: its operand is a
// `reflect.Type | reflect.TypeView` value, each `extends` bound is an
// interface written in full, and a bounded statement has an `else` block,
// which diverges.

interface Named { name string }

interface Holder<T> { held T }

class Wrapper<T> { inner T }

function ident<T>(v: T) -> T throws never { v }

// An `unknown` is not a type operand. A value the checker has not typed may
// not be erased into the frame to fail at run time: narrow it first with
// `match` or `is` (pinned in `ns_scoped_type_bindings`).
function unknown_operand(x: unknown) -> bool {
    let type T = x;
    reflect.Type.of<T>() == reflect.Type.of<int>()
}

// An open operand whose variable can never solve: it carries the literal
// `1` and the operand contract, which no type meets, so the contract is
// reported at `v` (E0001).
function operand_solved_to_int() -> int {
    let v = ident(1);
    let type T = v;
    1
}

// The operand contract is a `reflect.Type | reflect.TypeView` (or a builder's
// pending type); a nullable one is not narrowed yet.
function nullable_operand(t: reflect.Type?) -> int {
    let type T = t;
    1
}

// A bound is judged like a generic parameter's: a class is not an interface
// (E0145), an unknown name is unresolved (E0002), the reflection-only
// function interface is not a bound (E0154), and a `_` has nothing to infer
// from. Each bound is reported on its own; the binding keeps the rest.
function bounds_are_interfaces(t: reflect.Type) -> int {
    let type A extends Wrapper<int> = t else { return 0 };
    let type B extends Missing = t else { return 0 };
    let type C extends reflect.AnyFunction = t else { return 0 };
    let type D extends Named & Holder<_> = t else { return 0 };
    1
}

// The bounds and the `else` block are judged before `T` exists, so neither
// can name it.
function the_binding_is_not_in_scope_yet(t: reflect.Type) -> int {
    let type T extends Holder<T> = t else {
        let unseen: T? = null;
        return 0
    };
    1
}

// A miss runs the `else` block in place of the rest of the block, so it must
// diverge, as in `let … else`.
function else_must_diverge(t: reflect.Type) -> int {
    let type T extends Named = t else { 0 };
    1
}

// The bounds are checked when the statement runs, so a bounded statement
// must say what a miss does: without an `else` block it is an error (E0111),
// as a refutable `let` pattern is. A miss never throws.
function bounded_needs_else(t: reflect.Type) -> int throws never {
    let type T extends Named = t;
    let type U extends Named & Holder<int> = t;
    1
}

// Every miss runs the `else` block, so a bounded statement adds nothing to
// the throws a declared clause must name.
function bound_check_does_not_throw(t: reflect.Type) -> int throws never {
    let type T extends Named = t else { return 0 };
    1
}
