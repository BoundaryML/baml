// A6 (S15 ledger): `reflect` is its own PACKAGE and already resolved
// through the ordinary package-prefix step; the real gap was the
// `type` primitive's member home - reflection members (BEP-039) live
// on `class baml.TypeValue`, now a receiver-class table row, so
// `reflect.type_of<T>().to_string()` types end to end.
function f() -> string throws never {
    let s = reflect.type_of<int>().to_string();
     // ^ string
    s
}
