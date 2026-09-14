// Spec Subtyping/Variance: generics are INVARIANT - `Pack<int>` is not
// a subtype of `Pack<int | string>`, so their join must keep BOTH
// members (the negative-space counterpart of the absorption fixtures).
class Pack<T> {
    value T
}

function f(a: Pack<int>, b: Pack<int | string>, c: bool) -> int {
    let u = if (c) { a } else { b };
     // ^ user.Pack<int> | user.Pack<int | string>
    1
}
