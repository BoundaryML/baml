// A6 (S15 ledger): the `type` package is resolved through the ordinary
// package-prefix step. The `type` primitive's reflection members (BEP-039)
// live on `class baml.TypeValue`, now a receiver-class table row, so
// `type.of<T>().to_string()` types end to end.
function f() -> string throws never {
    let s = type.of<int>().to_string();
     // ^ string
    s
}
