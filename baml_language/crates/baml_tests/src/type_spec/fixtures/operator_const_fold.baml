// literal types are closed under builtin operators
function cf_add() -> 3n throws never {
    1n + 2n
}
function cf_mul() -> 12n throws never {
    3n * 4n
}
function cf_less() -> true throws never {
    1n < 2n
}
function cf_int_add() -> 3 throws never {
    1 + 2
}
function cf_binding_widens() -> bigint throws never {
    let x = 1n + 2n
     // ^ bigint
    x
}
