// pending: S6 literal widening at binding sites
function f() -> int {
    let x = 1;
     // ^ int
    x
 // ^ int
}
