function add(a: int, b: int) -> int {
    a + b
}

function g() -> int {
    let r = add(1, 2);
     // ^ int
      //    ^^^^^^^^^ int
    r
}
