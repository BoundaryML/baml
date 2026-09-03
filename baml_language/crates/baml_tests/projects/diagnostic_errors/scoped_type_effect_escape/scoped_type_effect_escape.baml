// What LEAVES a block is erased of the block's scoped parameter: the effect
// the enclosing function publishes, and the compiler-derived copy of it a
// stashed violation quotes. A clause the author WROTE inside the block is
// quoted back verbatim, at the line that spells it, where the name is in
// scope.

class Boom<T> { payload T }

function risky<T>(v: T, fail: bool) -> int throws Boom<T> {
    if (fail) { throw Boom<T> { payload: v } }
    0
}

// The effect this function publishes is the erased `Boom<unknown>`, never the
// block-scoped `Boom<Out>`; the declared `never` contract reads it back.
function publishes_the_erased_effect() -> int throws never {
    let t = reflect.Type.of<string>()
    type Out = unreflect(t)
    risky<Out>("x", false)
}

// One program, both spellings: the lambda's own `Boom<Out>` as written, and
// the owner's published `Boom<unknown>`.
function lambda_clause_is_quoted_as_written(t: reflect.Type) -> unknown throws never {
    type Out = unreflect(t)
    let f = (v: int) -> int throws Boom<Out> { throw "plain" }
    f(1)
}
