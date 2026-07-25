// End-to-end test for catch_all keyword: parsing, HIR, TIR, codegen

function MayFailIntOrString(x: int) -> string {
    match (x) {
        0 => throw "string error",
        1 => throw 42,
        _ => "ok"
    }
}

// Non-exhaustive catch — partial handling is fine
function PartialCatch(x: int) -> string {
    MayFailIntOrString(x) catch (e) {
        string => "caught string"
    }
}

// Exhaustive catch_all — wildcard covers everything
function CatchAllWildcard(x: int) -> string {
    MayFailIntOrString(x) catch_all (e) {
        _ => "caught all"
    }
}

// catch_all with all types explicitly handled
function CatchAllTyped(x: int) -> string {
    MayFailIntOrString(x) catch_all (e) {
        string => "caught string",
        int => "caught int"
    }
}

// Chained: catch handles string, catch_all handles rest
function CatchThenCatchAll(x: int) -> string {
    MayFailIntOrString(x) catch (e) {
        string => "caught string"
    } catch_all (e2) {
        _ => "caught rest"
    }
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- semantic_tokens
// catch_all_keyword.baml:1:1 (comment) len=68 "// End-to-end test for catch_all keyword: parsing, HIR, TIR, codegen"
// catch_all_keyword.baml:3:1 (keyword) len=8 "function"
// catch_all_keyword.baml:3:10 (function) [declaration] len=18 "MayFailIntOrString"
// catch_all_keyword.baml:3:29 (parameter) [declaration] len=1 "x"
// catch_all_keyword.baml:3:32 (type) [defaultLibrary] len=3 "int"
// catch_all_keyword.baml:3:40 (type) [defaultLibrary] len=6 "string"
// catch_all_keyword.baml:4:5 (keyword) len=5 "match"
// catch_all_keyword.baml:4:12 (parameter) len=1 "x"
// catch_all_keyword.baml:5:9 (number) len=1 "0"
// catch_all_keyword.baml:5:14 (keyword) len=5 "throw"
// catch_all_keyword.baml:5:20 (string) len=14 "\"string error\""
// catch_all_keyword.baml:6:9 (number) len=1 "1"
// catch_all_keyword.baml:6:14 (keyword) len=5 "throw"
// catch_all_keyword.baml:6:20 (number) len=2 "42"
// catch_all_keyword.baml:7:14 (string) len=4 "\"ok\""
// catch_all_keyword.baml:11:1 (comment) len=52 "// Non-exhaustive catch — partial handling is fine"
// catch_all_keyword.baml:12:1 (keyword) len=8 "function"
// catch_all_keyword.baml:12:10 (function) [declaration] len=12 "PartialCatch"
// catch_all_keyword.baml:12:23 (parameter) [declaration] len=1 "x"
// catch_all_keyword.baml:12:26 (type) [defaultLibrary] len=3 "int"
// catch_all_keyword.baml:12:34 (type) [defaultLibrary] len=6 "string"
// catch_all_keyword.baml:13:5 (function) len=18 "MayFailIntOrString"
// catch_all_keyword.baml:13:24 (parameter) len=1 "x"
// catch_all_keyword.baml:13:27 (keyword) len=5 "catch"
// catch_all_keyword.baml:13:34 (parameter) [declaration] len=1 "e"
// catch_all_keyword.baml:14:9 (type) [defaultLibrary] len=6 "string"
// catch_all_keyword.baml:14:19 (string) len=15 "\"caught string\""
// catch_all_keyword.baml:18:1 (comment) len=54 "// Exhaustive catch_all — wildcard covers everything"
// catch_all_keyword.baml:19:1 (keyword) len=8 "function"
// catch_all_keyword.baml:19:10 (function) [declaration] len=16 "CatchAllWildcard"
// catch_all_keyword.baml:19:27 (parameter) [declaration] len=1 "x"
// catch_all_keyword.baml:19:30 (type) [defaultLibrary] len=3 "int"
// catch_all_keyword.baml:19:38 (type) [defaultLibrary] len=6 "string"
// catch_all_keyword.baml:20:5 (function) len=18 "MayFailIntOrString"
// catch_all_keyword.baml:20:24 (parameter) len=1 "x"
// catch_all_keyword.baml:20:27 (keyword) len=9 "catch_all"
// catch_all_keyword.baml:20:38 (parameter) [declaration] len=1 "e"
// catch_all_keyword.baml:21:14 (string) len=12 "\"caught all\""
// catch_all_keyword.baml:25:1 (comment) len=46 "// catch_all with all types explicitly handled"
// catch_all_keyword.baml:26:1 (keyword) len=8 "function"
// catch_all_keyword.baml:26:10 (function) [declaration] len=13 "CatchAllTyped"
// catch_all_keyword.baml:26:24 (parameter) [declaration] len=1 "x"
// catch_all_keyword.baml:26:27 (type) [defaultLibrary] len=3 "int"
// catch_all_keyword.baml:26:35 (type) [defaultLibrary] len=6 "string"
// catch_all_keyword.baml:27:5 (function) len=18 "MayFailIntOrString"
// catch_all_keyword.baml:27:24 (parameter) len=1 "x"
// catch_all_keyword.baml:27:27 (keyword) len=9 "catch_all"
// catch_all_keyword.baml:27:38 (parameter) [declaration] len=1 "e"
// catch_all_keyword.baml:28:9 (type) [defaultLibrary] len=6 "string"
// catch_all_keyword.baml:28:19 (string) len=15 "\"caught string\""
// catch_all_keyword.baml:29:9 (type) [defaultLibrary] len=3 "int"
// catch_all_keyword.baml:29:16 (string) len=12 "\"caught int\""
// catch_all_keyword.baml:33:1 (comment) len=56 "// Chained: catch handles string, catch_all handles rest"
// catch_all_keyword.baml:34:1 (keyword) len=8 "function"
// catch_all_keyword.baml:34:10 (function) [declaration] len=17 "CatchThenCatchAll"
// catch_all_keyword.baml:34:28 (parameter) [declaration] len=1 "x"
// catch_all_keyword.baml:34:31 (type) [defaultLibrary] len=3 "int"
// catch_all_keyword.baml:34:39 (type) [defaultLibrary] len=6 "string"
// catch_all_keyword.baml:35:5 (function) len=18 "MayFailIntOrString"
// catch_all_keyword.baml:35:24 (parameter) len=1 "x"
// catch_all_keyword.baml:35:27 (keyword) len=5 "catch"
// catch_all_keyword.baml:35:34 (parameter) [declaration] len=1 "e"
// catch_all_keyword.baml:36:9 (type) [defaultLibrary] len=6 "string"
// catch_all_keyword.baml:36:19 (string) len=15 "\"caught string\""
// catch_all_keyword.baml:37:7 (keyword) len=9 "catch_all"
// catch_all_keyword.baml:37:18 (parameter) [declaration] len=2 "e2"
// catch_all_keyword.baml:38:14 (string) len=13 "\"caught rest\""
