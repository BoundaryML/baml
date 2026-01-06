function CreateMap() -> map<string, string> {
    { "hello": "world" }
}

function CreateMapJSON() -> map<string, string> {
    {"hello": "world"}
}

function UseMap() -> string {
    let map = CreateMap();
    map["hello"]
}

function UseMapNoKey() -> string {
    let map = CreateMapJSON();
    map["world"]
}

function UseMapContains() -> string {
    let map = CreateMapJSON();
    if (map.has("hello")) {
        map["hello"]
    } else {
        "hi"
    }
}

function Len() -> int {
    let map = CreateMap();
    map.length()
}

//----
//- diagnostics
// Error: Expected `map<string, string>`, found `map<"hello", "world">`
//    ╭─[ maps_maps.baml:5:50 ]
//    │
//  5 │ ╭─▶ function CreateMapJSON() -> map<string, string> {
//  6 │ ├─▶     {"hello": "world"}
//    │ │                            
//    │ ╰──────────────────────────── Expected `map<string, string>`, found `map<"hello", "world">`
//    │     
//    │     Note: Error code: E0001
// ───╯
// Error: Expected `map<string, string>`, found `map<"hello", "world">`
//    ╭─[ maps_maps.baml:1:46 ]
//    │
//  1 │ ╭─▶ function CreateMap() -> map<string, string> {
//  2 │ ├─▶     { "hello": "world" }
//    │ │                              
//    │ ╰────────────────────────────── Expected `map<string, string>`, found `map<"hello", "world">`
//    │     
//    │     Note: Error code: E0001
// ───╯
