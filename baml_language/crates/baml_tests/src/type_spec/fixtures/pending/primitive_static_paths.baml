// pending: A3 primitive-name static paths (S15 ledger, 610 nodes)
//
// `float.nan()` / `int.max_value()`: a primitive KEYWORD head maps
// through the receiver-class table (`float` -> `baml.Float`) into the
// existing S11 type-qualified static-method path.
function f() -> float throws never {
    let n = float.nan();
     // ^ float
    let m = int.max_value();
     // ^ int
    n
}
