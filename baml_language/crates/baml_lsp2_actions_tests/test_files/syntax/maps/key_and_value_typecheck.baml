// For now, errors with 'map keys must be string literals'
function CreateMapExpr() -> map<int, string> {
    { (1 + 1): "2" }
}

function UseMapContains() -> string {
    let map = CreateMapExpr();
    if (map.has(1)) {
        map[1]
    } else {
        "hi"
    }
}

// wrong input type, wrong output type.
function UseMapTypecheck() -> int {
    let map = CreateMapExpr();
    map["string"]
}

// error: Error validating: Map keys must be string literals
//   -->  maps/key_and_value_typecheck.baml:3
//    |
//  2 | function CreateMapExpr() -> map<int, string> {
//  3 |     { (1 + 1): "2" }
//    |
// error: Error validating: Type mismatch in argument, expected: string, got: int
//   -->  maps/key_and_value_typecheck.baml:8
//    |
//  7 |     let map = CreateMapExpr();
//  8 |     if (map.has(1)) {
//    |
// error: Error validating: Map access must be a string
//   -->  maps/key_and_value_typecheck.baml:9
//    |
//  8 |     if (map.has(1)) {
//  9 |         map[1]
//    |
// error: Error validating: Return type mismatch: function return type is int but got string
//   -->  maps/key_and_value_typecheck.baml:18
//    |
// 17 |     let map = CreateMapExpr();
// 18 |     map["string"]
//    |

//----
//- diagnostics
// E0010
//
//   × unexpected token
//    ╭─[maps_key_and_value_typecheck.baml:3:14]
//  3 │     { (1 + 1): "2" }
//    ·              ┬
//    ·              ╰── expected `expression`, found `':'`
//    ╰────
// E0001
//
//   × mismatched types
//     ╭─[maps_key_and_value_typecheck.baml:18:9]
//  18 │     map["string"]
//     ·         ────┬───
//     ·             ╰── expected `int`, found `"string"`
//     ╰────
// E0001
//
//   × mismatched types
//     ╭─[maps_key_and_value_typecheck.baml:16:35]
//  16 │ ╭─▶ function UseMapTypecheck() -> int {
//  17 │ │       let map = CreateMapExpr();
//  18 │ │       map["string"]
//  19 │ ├─▶ }
//     · ╰──── expected `int`, found `string`
//     ╰────
// E0067
//
//   × map keys must be `string`; got `int`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//    ╭─[maps_key_and_value_typecheck.baml:2:29]
//  2 │ function CreateMapExpr() -> map<int, string> {
//    ·                             ────────────────
//    ╰────
