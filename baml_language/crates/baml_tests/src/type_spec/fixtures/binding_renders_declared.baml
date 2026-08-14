// Ruling 3 (S15): a binding's recorded type is the pattern's DECLARED
// type. A union annotation parses as an OR of type ascriptions - the
// recorded type is the written union.
//
// Flow narrowing affects USES, not the binding's recorded type: with
// `let v: int | string = 3`, the binding is `int | string` (what the
// user wrote), while reads inside a narrowed region may still see the
// narrowed type through the flow overlay.
function f() -> int throws never {
    let v: int | string = 3;
     // ^ int | string
    0
}
