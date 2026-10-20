// `void` is another spelling of `null`, the unit type (BEP-079).
//
// One type, not two mutually-assignable ones: `void` lowers to `null`
// wherever it is written, so it is valid in every type position and
// renders as `null`.
function g() -> void throws never {
    null
}

function f() -> int throws never {
    let v: void = null;
     // ^ null
    0
}
