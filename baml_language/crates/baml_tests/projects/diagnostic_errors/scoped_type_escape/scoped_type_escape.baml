// Nothing typed by a block-scoped `type T = …` binding may be observable
// outside the block that binds it (E0171): the binding re-executes on every
// pass through its block and may bind `T` differently each time. A value
// leaves only through a type that does not mention `T`, and a thrown type an
// inferred `throws` clause or a `catch` outside the block would publish is
// refused at the throw.

class Boom<T> { payload T }

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

// The block's value has type `T`, and nothing says what it may leave as.
function unannotated_block_value(t: reflect.Type) -> unknown {
    let escaped = {
        type T = unreflect(t)
        as_t<T>(1)
    }
    escaped
}

// The body's expected type `int` does not admit a `T`.
function body_value_against_a_narrower_return(t: reflect.Type) -> int {
    type T = unreflect(t)
    as_t<T>(1)
}

// An omitted clause would publish `Boom<T>` as this function's surface.
function inferred_throws_publish_the_binding(t: reflect.Type) -> int {
    type T = unreflect(t)
    risky<T>(as_t<T>("x"), false)
}

// A lambda with no clause of its own publishes its body's throws as its type.
function lambda_infers_its_own_clause(t: reflect.Type) -> unknown throws never {
    let f = () -> {
        type T = unreflect(t)
        risky<T>(as_t<T>("x"), false)
    }
    f
}

// A catch outside the block would bind the fact past the block.
function catch_outside_the_block(t: reflect.Type) -> int throws never {
    let n = {
        type T = unreflect(t)
        risky<T>(as_t<T>("x"), true)
    } catch (e) {
        _ => 0
    }
    n
}

// A list declared outside the block cannot take `T` as its element type:
// it would carry `T` after the block, and after a rebinding.
function outer_container_takes_the_binding(t: reflect.Type) -> unknown {
    let xs = []
    {
        type T = unreflect(t)
        xs.push(as_t<T>(1))
    }
    xs
}

// With no clause, a thrown `T[]` cannot be published, and it is never
// relaxed for the author: its nearest relaxation is `unknown` (an invariant
// `T[]` collapses whole, never to `unknown[]`), which the clause must say.
function thrown_list_without_a_clause(t: reflect.Type) -> string {
    type T = unreflect(t)
    let v: T = as_t<T>(1)
    let a: T[] = [v]
    throw a
}

// `unknown[]` is NOT a relaxation of `T[]`, nor `Boom<unknown>` of
// `Boom<T>`: their arguments are invariant, so the clause must say `unknown`.
// Each is judged at the throw against the clause as written.
function list_relaxed_wrongly(t: reflect.Type) -> string throws unknown[] | string {
    type T = unreflect(t)
    let v: T = as_t<T>(1)
    let a: T[] = [v]
    throw a
}

function boom_relaxed_wrongly(t: reflect.Type) -> int throws Boom<unknown> | string {
    type T = unreflect(t)
    risky<T>(as_t<T>("x"), true)
}

function ident<T>(v: T) -> T throws never { v }

// `throws unknown` is imprecise when something closer exists: a thrown
// `() -> T throws never` relaxes to `() -> unknown throws never` (the return
// is covariant), and that is what the clause should say.
function imprecise_unknown_clause(t: reflect.Type) -> int throws unknown {
    type T = unreflect(t)
    let v: T = as_t<T>(1)
    throw () -> { ident<T>(v) }
}

// A `return` leaves every block of its lambda at once, and a lambda with no
// declared return type would publish `T` as its result.
function return_from_an_inferred_lambda(t: reflect.Type) -> unknown throws never {
    let f = () -> {
        type T = unreflect(t)
        return as_t<T>("x")
    }
    f
}

// The frame's declared `int` does not admit a `T`, so the return escapes.
function return_against_a_narrower_type(t: reflect.Type) -> int {
    type T = unreflect(t)
    return as_t<T>(1)
}
