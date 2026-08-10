// array-pattern ascriptions resolve aliases before the element demand
// tir: fails
type AaaInts = int[]
function aaa_any(u: unknown) -> int throws never {
    let [let a, let b]: AaaInts = u
    let r = a + b
     // ^ int
    r
}
