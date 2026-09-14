// Value generics through lambdas, direction 2: nothing but the
// lambda's RETURN constrains `T` - the body's inferred type is the
// lower bound, fresh literals widening at resolution (ruling 1), so
// the call produces string[], not "s"[].
function build<T>(f: (x: int) -> T) -> T[] {
    [f(1)]
}

function g() -> string[] {
    let r = build((x) -> { "s" });
     // ^ string[]
    r
}
