function g() -> int {
    let base = 10;
    let f = (x: int) -> int throws never { base }
                                       //  ^^^^ int
    f(1)
}
