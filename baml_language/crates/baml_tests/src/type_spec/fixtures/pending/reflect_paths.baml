// pending: A6 reflect.* path resolution (S15 ledger, 121 nodes)
//
// The `reflect` root resolves (TIR does; our value-path fallback does
// not) - `reflect.type_of<T>()` yields a `baml.TypeValue`.
function f() -> string throws never {
    let s = reflect.type_of<int>().to_string();
     // ^ string
    s
}
