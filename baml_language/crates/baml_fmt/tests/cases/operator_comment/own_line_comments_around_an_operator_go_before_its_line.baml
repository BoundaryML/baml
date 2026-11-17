function f(a: int, b: int, c: int) -> int {
    let l = a - b -
/*C2*/
 c - b;
    let e = a == b /*C1*/  - 
//C2
c - a;
    l + e
}
