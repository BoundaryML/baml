function f(v: int | string, d: string) -> string {
    let r = match (v) {
        let s: string => s,
              //         ^ string
        let n: int => d,
    };
    r
 // ^ string
}
