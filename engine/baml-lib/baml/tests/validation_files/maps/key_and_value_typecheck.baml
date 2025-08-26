// For now, errors with 'map keys must be string literals'
fn CreateMapExpr() -> map<int, string> {
    { (1 + 1): "2" }
}

fn UseMapContains() -> string {
    let map = CreateMapExpr();
    if (map.contains(1)) {
        map[1]
    } else {
        "hi"
    }
}

// wrong input type, wrong output type.
fn UseMapTypecheck() -> int {
    let map = CreateMapExpr();
    map["string"]
}

// warning: Workflow functions are experimental, and will break in the future.
//   -->  maps/key_and_value_typecheck.baml:2
//    | 
//  1 | // For now, errors with 'map keys must be string literals'
//  2 | fn CreateMapExpr() -> map<int, string> {
//    | 
// warning: Workflow functions are experimental, and will break in the future.
//   -->  maps/key_and_value_typecheck.baml:6
//    | 
//  5 | 
//  6 | fn UseMapContains() -> string {
//    | 
// warning: Workflow functions are experimental, and will break in the future.
//   -->  maps/key_and_value_typecheck.baml:16
//    | 
// 15 | // wrong input type, wrong output type.
// 16 | fn UseMapTypecheck() -> int {
//    | 
// error: Error validating: Map keys must be string literals
//   -->  maps/key_and_value_typecheck.baml:3
//    | 
//  2 | fn CreateMapExpr() -> map<int, string> {
//  3 |     { (1 + 1): "2" }
//    | 
// error: Error validating: Type mismatch in argument, expected: string, got: int
//   -->  maps/key_and_value_typecheck.baml:8
//    | 
//  7 |     let map = CreateMapExpr();
//  8 |     if (map.contains(1)) {
//    | 
// error: Error validating: Map access must be a string
//   -->  maps/key_and_value_typecheck.baml:9
//    | 
//  8 |     if (map.contains(1)) {
//  9 |         map[1]
//    | 
// error: Error validating: Return type mismatch: function return type is int but got string
//   -->  maps/key_and_value_typecheck.baml:18
//    | 
// 17 |     let map = CreateMapExpr();
// 18 |     map["string"]
//    | 
