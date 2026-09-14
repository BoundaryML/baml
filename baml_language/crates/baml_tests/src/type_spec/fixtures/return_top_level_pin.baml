// The return expectation participates in generic argument inference:
// the expectation reaches the instantiation var as an upper bound
// (r-a check_call_arguments unifies expected_output with formal_output
// before args commit), and the literal-widening default never
// overrides it, so a literal expectation pins T while unconstrained
// instantiations still widen
function rtl_id<T>(x: T) -> T throws never {
    x
}
function rtl_wrap<T>(x: T) -> T[] throws never {
    [x]
}
function rtl_pins() -> 42 throws never {
    rtl_id(42)
 // ^^^^^^^^^^ 42
}
function rtl_nested_widens() -> int[] throws never {
    let xs = rtl_wrap(7)
     // ^^ int[]
    xs
}
