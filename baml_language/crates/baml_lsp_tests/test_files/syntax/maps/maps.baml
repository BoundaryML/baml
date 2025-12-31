function CreateMap() -> map<string, string> {
    { hello "world" }
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
// Error: Expected map<string, string>, found void
//    ╭─[ 0:1:1 ]
//    │
//  1 │ function CreateMap() -> map<string, string> {
//    │ │ 
//    │ ╰─ Expected map<string, string>, found void
//    │ 
//    │ Note: Error code: E0001
// ───╯
// Error: Type map<string, string> is not indexable
//     ╭─[ 0:16:5 ]
//     │
//  16 │     map["world"]
//     │     ──────┬─────  
//     │           ╰─────── Type map<string, string> is not indexable
//     │ 
//     │ Note: Error code: E0008
// ────╯
// Error: Unknown variable hello
//    ╭─[ 0:2:7 ]
//    │
//  2 │     { hello "world" }
//    │       ──┬──  
//    │         ╰──── Unknown variable hello
//    │ 
//    │ Note: Error code: E0003
// ───╯
// Error: Type map<string, string> has no field 'has'
//     ╭─[ 0:21:9 ]
//     │
//  21 │     if (map.has("hello")) {
//     │         ───┬───  
//     │            ╰───── Type map<string, string> has no field 'has'
//     │ 
//     │ Note: Error code: E0007
// ────╯
// Error: Type map<string, string> is not indexable
//     ╭─[ 0:22:9 ]
//     │
//  22 │         map["hello"]
//     │         ──────┬─────  
//     │               ╰─────── Type map<string, string> is not indexable
//     │ 
//     │ Note: Error code: E0008
// ────╯
// Error: Expected string, found unknown | void
//    ╭─[ 0:1:1 ]
//    │
//  1 │ function CreateMap() -> map<string, string> {
//    │ │ 
//    │ ╰─ Expected string, found unknown | void
//    │ 
//    │ Note: Error code: E0001
// ───╯
// Error: Type map<string, string> has no field 'length'
//     ╭─[ 0:29:27 ]
//     │
//  29 │ ╭─▶     let map = CreateMap();
//  30 │ ├─▶     map.length()
//     │ │                      
//     │ ╰────────────────────── Type map<string, string> has no field 'length'
//     │     
//     │     Note: Error code: E0007
// ────╯
// Error: Type map<string, string> is not indexable
//     ╭─[ 0:11:5 ]
//     │
//  11 │     map["hello"]
//     │     ──────┬─────  
//     │           ╰─────── Type map<string, string> is not indexable
//     │ 
//     │ Note: Error code: E0008
// ────╯
