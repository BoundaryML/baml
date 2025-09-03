fn CreateMap() -> map<string, string> {
    { hello "world" }
}

fn CreateMapJSON() -> map<string, string> {
    {"hello": "world"}
}

fn UseMap() -> string {
    let map = CreateMap();
    map["hello"]
}

fn UseMapNoKey() -> string {
    let map = CreateMapJSON();
    map["world"]
}

fn UseMapContains() -> string {
    let map = CreateMapJSON();
    if (map.contains("hello")) {
        map["hello"]
    } else {
        "hi"
    }
}

fn Len() -> int {
    let map = CreateMap();
    map.len()
}

// warning: Workflow functions are experimental, and will break in the future.
//   -->  maps/maps.baml:1
//    | 
//    | 
//  1 | fn CreateMap() -> map<string, string> {
//    | 
// warning: Workflow functions are experimental, and will break in the future.
//   -->  maps/maps.baml:5
//    | 
//  4 | 
//  5 | fn CreateMapJSON() -> map<string, string> {
//    | 
// warning: Workflow functions are experimental, and will break in the future.
//   -->  maps/maps.baml:9
//    | 
//  8 | 
//  9 | fn UseMap() -> string {
//    | 
// warning: Workflow functions are experimental, and will break in the future.
//   -->  maps/maps.baml:14
//    | 
// 13 | 
// 14 | fn UseMapNoKey() -> string {
//    | 
// warning: Workflow functions are experimental, and will break in the future.
//   -->  maps/maps.baml:19
//    | 
// 18 | 
// 19 | fn UseMapContains() -> string {
//    | 
// warning: Workflow functions are experimental, and will break in the future.
//   -->  maps/maps.baml:28
//    | 
// 27 | 
// 28 | fn Len() -> int {
//    | 
