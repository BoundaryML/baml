// B-1788: a method receiver demands an operator result before finish.
// Resolve the pending operator from its operand's bounds so method
// resolution and the call plan are recorded during the body walk.
function id<T>(x: T) -> T { x }

class Box<T> { value: T }

function generic_function(x: float) -> float {
    let r = (id(x) * 100.0).round();
     // ^ float
    r
}

function generic_class(n: int) -> int {
    let r = (-Box { value: n }.value).abs();
     // ^ int
    r
}

function untyped_lambda(s: string) -> string {
    let g = (a) -> { a };
    let r = (g(s) + " ").trim();
     // ^ string
    r
}

// The accumulator is bounded by its own callback output; the cycle must break.
function generic_method(values: int[]) -> int {
    let r = (-values.reduce((sum, n) -> { sum + n }, 0)).abs();
     // ^ int
    r
}
