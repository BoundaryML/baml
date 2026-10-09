// Native backend fixture: bigint arithmetic, comparison, bits, conversion,
// methods and JSON.

function arith(a: bigint, b: bigint) -> string {
    (a + b).to_string() + " " + (a - b).to_string() + " " + (a * b).to_string()
}

function divide(a: bigint, b: bigint) -> string {
    (a / b).to_string() + " " + (a % b).to_string()
}

function compare(a: bigint, b: bigint) -> string {
    (a < b).to_string() + (a <= b).to_string() + (a > b).to_string() + (a >= b).to_string()
        + (a == b).to_string() + (a != b).to_string()
}

function bits(a: bigint, b: bigint) -> string {
    (a & b).to_string() + " " + (a | b).to_string() + " " + (a ^ b).to_string() + " "
        + (a << 3).to_string() + " " + (a >> b).to_string()
}

function mixed(a: bigint, n: int) -> string {
    (a + n).to_string() + " " + (n * a).to_string() + " " + (a << n).to_string() + " "
        + (n - a).to_string() + " " + (a % n).to_string()
}

function literal() -> string {
    (123456789012345678901234567890n * -2n + 1n).to_string()
}

function narrow(a: bigint) -> int {
    a.to_int()
}

function widen(n: int) -> string {
    let x: bigint = 0n;
    x += n;
    (x * x - 1).to_string()
}

function negate(a: bigint) -> string {
    (-a).to_string() + " " + a.abs().to_string()
}

function methods(a: bigint, b: bigint) -> string {
    a.pow(b).to_string() + " " + a.isqrt().to_string() + " " + a.ilog(b).to_string()
}

function parse(s: string) -> string {
    bigint.parse(s).to_string()
}

function shift_wide(n: int) -> int {
    (1n << n).to_string().length()
}

function json_out(a: bigint) -> string {
    baml.json.to_string(a) + " " + baml.json.to_string([a, 1n])
}

function json_in(raw: string) -> string {
    baml.json.deserialize<bigint>(raw).to_string()
}
