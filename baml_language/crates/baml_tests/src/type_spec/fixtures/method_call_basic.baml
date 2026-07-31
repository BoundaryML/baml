// Instance methods on builtin containers: the receiver pins the class
// generics, `self` types through the builtin bridge (Array<T> IS T[]),
// and method generics resolve from lambda returns.
function f(xs: int[]) -> int? {
    let n = xs.push(4);
     // ^ int
    let m = xs.map((x) -> { x });
     // ^ int[]
    xs.at(0)
 // ^^^^^^^^ int | null
}
