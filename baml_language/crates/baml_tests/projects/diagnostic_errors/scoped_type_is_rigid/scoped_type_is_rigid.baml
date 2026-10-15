// An unbounded `let type T = …` binding is a rigid type parameter: a value
// has type `T` only if it came from a `T`-typed source, and a bounded slot is
// an ordinary static obligation that an unbounded `T` does not satisfy.
// Nothing is deferred to a runtime gate. (The bounded form,
// `let type T extends I = …`, checks `I` where it binds and then satisfies
// it: `ns_let_type_bounds`.)

interface Named { name string }

class Boom<T> { payload T }

function needs_bound<A extends Named>() -> string { "ok" }

function echo<T>(v: T) -> T throws never { v }

function risky<T>(v: T, fail: bool) -> int throws Boom<T> {
    if (fail) { throw Boom<T> { payload: v } }
    0
}

function as_t<T>(v: unknown) -> T throws string {
    match (v) {
        let x: T => x,
        _ => throw "not a T",
    }
}

// `T` carries no bound evidence, whatever the runtime type implements.
function unbounded_binding_fails_a_bound(t: reflect.Type) -> string {
    let type T = t;
    needs_bound<T>()
}

// A literal is not a `T`.
function literal_is_not_a_t(t: reflect.Type) -> unknown {
    let type T = t;
    echo<T>(42)
}

// A declared clause judges the scoped contribution as written, where the
// name is in scope.
function declared_clause_reads_the_binding(t: reflect.Type) -> int throws string {
    let type T = t;
    risky<T>(as_t<T>("x"), false)
}
