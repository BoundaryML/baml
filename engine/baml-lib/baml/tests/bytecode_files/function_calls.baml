// Functions that call other functions
function Add(x: int, y: int) -> int {
    x
}

function Multiply(x: int, y: int) -> int {
    y
}

function Calculate(a: int, b: int, c: int) -> int {
    let sum = Add(a, b);
    let product = Multiply(sum, c);
    product
}

function Nested(x: int) -> int {
    Add(Multiply(x, 2), Add(x, 1))
}