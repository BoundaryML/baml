function cmp(a: int, b: int) -> bool {
    let lt = a < b;
     // ^^ bool
    let eq = a == b;
     // ^^ bool
    let ne = !eq;
     // ^^ bool
    lt && ne
 // ^^^^^^^^ bool
}
