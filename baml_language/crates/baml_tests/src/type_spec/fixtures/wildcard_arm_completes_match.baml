function f(v: int | string, a: int) -> int {
    let r = match (v) {
        let n: int => n,
                   // ^ int
        _ => a,
    };
    r
 // ^ int
}
