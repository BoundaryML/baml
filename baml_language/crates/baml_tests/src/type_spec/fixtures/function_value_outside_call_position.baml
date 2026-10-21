function inc(x: int) -> int throws never {
    x
}

function g() -> int {
    let c: (x: int) -> int throws never = inc
                                       // ^^^ (x: int) -> int throws never
    c(1)
 // ^^^^ int
}
