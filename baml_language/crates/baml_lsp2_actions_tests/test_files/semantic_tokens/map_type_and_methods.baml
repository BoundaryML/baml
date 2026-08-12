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
// <no-diagnostics-expected>
//
//- semantic_tokens
// map_type_and_methods.baml:1:1 (keyword) len=8 "function"
// map_type_and_methods.baml:1:10 (function) [declaration] len=9 "CreateMap"
// map_type_and_methods.baml:1:25 (type) [defaultLibrary] len=3 "map"
// map_type_and_methods.baml:1:28 (operator) len=1 "<"
// map_type_and_methods.baml:1:29 (type) [defaultLibrary] len=6 "string"
// map_type_and_methods.baml:1:37 (type) [defaultLibrary] len=6 "string"
// map_type_and_methods.baml:1:43 (operator) len=1 ">"
// map_type_and_methods.baml:2:7 (string) len=7 "\"hello\""
// map_type_and_methods.baml:2:16 (string) len=7 "\"world\""
// map_type_and_methods.baml:5:1 (keyword) len=8 "function"
// map_type_and_methods.baml:5:10 (function) [declaration] len=13 "CreateMapJSON"
// map_type_and_methods.baml:5:29 (type) [defaultLibrary] len=3 "map"
// map_type_and_methods.baml:5:32 (operator) len=1 "<"
// map_type_and_methods.baml:5:33 (type) [defaultLibrary] len=6 "string"
// map_type_and_methods.baml:5:41 (type) [defaultLibrary] len=6 "string"
// map_type_and_methods.baml:5:47 (operator) len=1 ">"
// map_type_and_methods.baml:6:6 (string) len=7 "\"hello\""
// map_type_and_methods.baml:6:15 (string) len=7 "\"world\""
// map_type_and_methods.baml:9:1 (keyword) len=8 "function"
// map_type_and_methods.baml:9:10 (function) [declaration] len=6 "UseMap"
// map_type_and_methods.baml:9:22 (type) [defaultLibrary] len=6 "string"
// map_type_and_methods.baml:10:5 (keyword) len=3 "let"
// map_type_and_methods.baml:10:9 (variable) [declaration] len=3 "map"
// map_type_and_methods.baml:10:13 (operator) len=1 "="
// map_type_and_methods.baml:10:15 (function) len=9 "CreateMap"
// map_type_and_methods.baml:11:5 (variable) len=3 "map"
// map_type_and_methods.baml:11:9 (string) len=7 "\"hello\""
// map_type_and_methods.baml:14:1 (keyword) len=8 "function"
// map_type_and_methods.baml:14:10 (function) [declaration] len=11 "UseMapNoKey"
// map_type_and_methods.baml:14:27 (type) [defaultLibrary] len=6 "string"
// map_type_and_methods.baml:15:5 (keyword) len=3 "let"
// map_type_and_methods.baml:15:9 (variable) [declaration] len=3 "map"
// map_type_and_methods.baml:15:13 (operator) len=1 "="
// map_type_and_methods.baml:15:15 (function) len=13 "CreateMapJSON"
// map_type_and_methods.baml:16:5 (variable) len=3 "map"
// map_type_and_methods.baml:16:9 (string) len=7 "\"world\""
// map_type_and_methods.baml:19:1 (keyword) len=8 "function"
// map_type_and_methods.baml:19:10 (function) [declaration] len=14 "UseMapContains"
// map_type_and_methods.baml:19:30 (type) [defaultLibrary] len=6 "string"
// map_type_and_methods.baml:20:5 (keyword) len=3 "let"
// map_type_and_methods.baml:20:9 (variable) [declaration] len=3 "map"
// map_type_and_methods.baml:20:13 (operator) len=1 "="
// map_type_and_methods.baml:20:15 (function) len=13 "CreateMapJSON"
// map_type_and_methods.baml:21:5 (keyword) len=2 "if"
// map_type_and_methods.baml:21:9 (variable) len=3 "map"
// map_type_and_methods.baml:21:13 (method) len=3 "has"
// map_type_and_methods.baml:21:17 (string) len=7 "\"hello\""
// map_type_and_methods.baml:22:9 (variable) len=3 "map"
// map_type_and_methods.baml:22:13 (string) len=7 "\"hello\""
// map_type_and_methods.baml:23:7 (keyword) len=4 "else"
// map_type_and_methods.baml:24:9 (string) len=4 "\"hi\""
// map_type_and_methods.baml:28:1 (keyword) len=8 "function"
// map_type_and_methods.baml:28:10 (function) [declaration] len=3 "Len"
// map_type_and_methods.baml:28:19 (type) [defaultLibrary] len=3 "int"
// map_type_and_methods.baml:29:5 (keyword) len=3 "let"
// map_type_and_methods.baml:29:9 (variable) [declaration] len=3 "map"
// map_type_and_methods.baml:29:13 (operator) len=1 "="
// map_type_and_methods.baml:29:15 (function) len=9 "CreateMap"
// map_type_and_methods.baml:30:5 (variable) len=3 "map"
// map_type_and_methods.baml:30:9 (method) len=6 "length"
