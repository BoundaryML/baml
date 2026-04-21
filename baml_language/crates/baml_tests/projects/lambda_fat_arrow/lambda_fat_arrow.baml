// Lambdas written with `=>` parse identically to `->`.
// The formatter normalizes `=>` to `->`.

// Fully annotated with fat arrow.
function test_annotated() -> int {
    let double = (x: int) => int { x * 2 }
    double(21)
}

// Zero-param fat arrow.
function test_zero_param() -> int {
    let constant = () => int { 42 }
    constant()
}

// Inferred return type with fat arrow.
function test_inferred() -> int {
    let double = (x: int) => { x * 2 }
    double(21)
}

// Multi-param fat arrow.
function test_multi_param() -> int {
    let add = (a: int, b: int) => int { a + b }
    add(20, 22)
}

// Generic fat arrow.
function test_generic() -> int {
    let identity = <T>(x: T) => T { x }
    identity(42)
}

// Fat arrow inside match arm — fat arrow of match vs fat arrow of lambda.
function test_in_match(x: int) -> int {
    match (x) {
        0 => {
            let inc = (y: int) => int { y + 1 }
            inc(10)
        }
        _ => 0
    }
}

// Mixed arrows in the same program.
function test_mixed() -> int {
    let a = (x: int) -> int { x + 1 }
    let b = (x: int) => int { x + 2 }
    a(0) + b(0)
}
