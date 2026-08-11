enum MapKey {
    A,
    B,
    C,
}

class Fields {
    e: map<MapKey, string>,
    l1: map<"literal", string>,
    l2: map<"one" | "two" | ("three" | "four"), string>,
}

function InOutEnumKey(i1: map<MapKey, string>, i2: map<MapKey, string>) -> map<MapKey, string> {
    client: "openai/gpt-4o"
    prompt: `
        Merge these: ${i1} ${i2}

        ${ctx.output_format}
    `
}

function InOutLiteralStringUnionMapKey(
    i1: map<"one" | "two" | ("three" | "four"), string>,
    i2: map<"one" | "two" | ("three" | "four"), string>,
) -> map<"one" | "two" | ("three" | "four"), string> {
    client: "openai/gpt-4o"
    prompt: `
        Merge these:

        ${i1}

        ${i2}

        ${ctx.output_format}
    `
}

//----
//- diagnostics
// E0067
//
//   × map keys must be `string`; got `MapKey`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     ╭─[map_literal_keys.baml:13:76]
//  13 │ function InOutEnumKey(i1: map<MapKey, string>, i2: map<MapKey, string>) -> map<MapKey, string> {
//     ·                                                                            ───────────────────
//     ╰────
// E0067
//
//   × map keys must be `string`; got `MapKey`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     ╭─[map_literal_keys.baml:13:27]
//  13 │ function InOutEnumKey(i1: map<MapKey, string>, i2: map<MapKey, string>) -> map<MapKey, string> {
//     ·                           ───────────────────
//     ╰────
// E0067
//
//   × map keys must be `string`; got `MapKey`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     ╭─[map_literal_keys.baml:13:52]
//  13 │ function InOutEnumKey(i1: map<MapKey, string>, i2: map<MapKey, string>) -> map<MapKey, string> {
//     ·                                                    ───────────────────
//     ╰────
// E0067
//
//   × map keys must be `string`; got `MapKey`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     ╭─[map_literal_keys.baml:13:96]
//  13 │ ╭─▶ function InOutEnumKey(i1: map<MapKey, string>, i2: map<MapKey, string>) -> map<MapKey, string> {
//  14 │ │       client: "openai/gpt-4o"
//  15 │ │       prompt: `
//  16 │ │           Merge these: ${i1} ${i2}
//  17 │ │
//  18 │ │           ${ctx.output_format}
//  19 │ │       `
//  20 │ ╰─▶ }
//     ╰────
// E0067
//
//   × map keys must be `string`; got `MapKey`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     ╭─[map_literal_keys.baml:13:1]
//  13 │ ╭─▶ function InOutEnumKey(i1: map<MapKey, string>, i2: map<MapKey, string>) -> map<MapKey, string> {
//  14 │ │       client: "openai/gpt-4o"
//  15 │ │       prompt: `
//  16 │ │           Merge these: ${i1} ${i2}
//  17 │ │
//  18 │ │           ${ctx.output_format}
//  19 │ │       `
//  20 │ ╰─▶ }
//     ╰────
// E0067
//
//   × map keys must be `string`; got `MapKey`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     ╭─[map_literal_keys.baml:13:27]
//  13 │ function InOutEnumKey(i1: map<MapKey, string>, i2: map<MapKey, string>) -> map<MapKey, string> {
//     ·                           ───────────────────
//     ╰────
// E0067
//
//   × map keys must be `string`; got `MapKey`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     ╭─[map_literal_keys.baml:13:52]
//  13 │ function InOutEnumKey(i1: map<MapKey, string>, i2: map<MapKey, string>) -> map<MapKey, string> {
//     ·                                                    ───────────────────
//     ╰────
// E0067
//
//   × map keys must be `string`; got `MapKey`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     ╭─[map_literal_keys.baml:13:96]
//  13 │ ╭─▶ function InOutEnumKey(i1: map<MapKey, string>, i2: map<MapKey, string>) -> map<MapKey, string> {
//  14 │ │       client: "openai/gpt-4o"
//  15 │ │       prompt: `
//  16 │ │           Merge these: ${i1} ${i2}
//  17 │ │
//  18 │ │           ${ctx.output_format}
//  19 │ │       `
//  20 │ ╰─▶ }
//     ╰────
// E0067
//
//   × map keys must be `string`; got `MapKey`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     ╭─[map_literal_keys.baml:13:27]
//  13 │ function InOutEnumKey(i1: map<MapKey, string>, i2: map<MapKey, string>) -> map<MapKey, string> {
//     ·                           ───────────────────
//     ╰────
// E0067
//
//   × map keys must be `string`; got `MapKey`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     ╭─[map_literal_keys.baml:13:52]
//  13 │ function InOutEnumKey(i1: map<MapKey, string>, i2: map<MapKey, string>) -> map<MapKey, string> {
//     ·                                                    ───────────────────
//     ╰────
// E0067
//
//   × map keys must be `string`; got `MapKey`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     ╭─[map_literal_keys.baml:13:1]
//  13 │ ╭─▶ function InOutEnumKey(i1: map<MapKey, string>, i2: map<MapKey, string>) -> map<MapKey, string> {
//  14 │ │       client: "openai/gpt-4o"
//  15 │ │       prompt: `
//  16 │ │           Merge these: ${i1} ${i2}
//  17 │ │
//  18 │ │           ${ctx.output_format}
//  19 │ │       `
//  20 │ ╰─▶ }
//     ╰────
// E0067
//
//   × map keys must be `string`; got `MapKey`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     ╭─[map_literal_keys.baml:13:76]
//  13 │ function InOutEnumKey(i1: map<MapKey, string>, i2: map<MapKey, string>) -> map<MapKey, string> {
//     ·                                                                            ───────────────────
//     ╰────
// E0067
//
//   × map keys must be `string`; got `MapKey`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     ╭─[map_literal_keys.baml:13:1]
//  13 │ ╭─▶ function InOutEnumKey(i1: map<MapKey, string>, i2: map<MapKey, string>) -> map<MapKey, string> {
//  14 │ │       client: "openai/gpt-4o"
//  15 │ │       prompt: `
//  16 │ │           Merge these: ${i1} ${i2}
//  17 │ │
//  18 │ │           ${ctx.output_format}
//  19 │ │       `
//  20 │ ╰─▶ }
//     ╰────
// E0067
//
//   × map keys must be `string`; got `MapKey`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//    ╭─[map_literal_keys.baml:8:8]
//  8 │     e: map<MapKey, string>,
//    ·        ───────────────────
//    ╰────
//
//- semantic_tokens
// map_literal_keys.baml:1:1 (keyword) len=4 "enum"
// map_literal_keys.baml:1:6 (enum) [declaration] len=6 "MapKey"
// map_literal_keys.baml:2:5 (enumMember) [declaration] len=1 "A"
// map_literal_keys.baml:3:5 (enumMember) [declaration] len=1 "B"
// map_literal_keys.baml:4:5 (enumMember) [declaration] len=1 "C"
// map_literal_keys.baml:7:1 (keyword) len=5 "class"
// map_literal_keys.baml:7:7 (class) [declaration] len=6 "Fields"
// map_literal_keys.baml:8:5 (property) [declaration] len=1 "e"
// map_literal_keys.baml:8:8 (type) [defaultLibrary] len=3 "map"
// map_literal_keys.baml:8:11 (operator) len=1 "<"
// map_literal_keys.baml:8:12 (enum) len=6 "MapKey"
// map_literal_keys.baml:8:20 (type) [defaultLibrary] len=6 "string"
// map_literal_keys.baml:8:26 (operator) len=1 ">"
// map_literal_keys.baml:9:5 (property) [declaration] len=2 "l1"
// map_literal_keys.baml:9:9 (type) [defaultLibrary] len=3 "map"
// map_literal_keys.baml:9:12 (operator) len=1 "<"
// map_literal_keys.baml:9:13 (string) len=9 "\"literal\""
// map_literal_keys.baml:9:24 (type) [defaultLibrary] len=6 "string"
// map_literal_keys.baml:9:30 (operator) len=1 ">"
// map_literal_keys.baml:10:5 (property) [declaration] len=2 "l2"
// map_literal_keys.baml:10:9 (type) [defaultLibrary] len=3 "map"
// map_literal_keys.baml:10:12 (operator) len=1 "<"
// map_literal_keys.baml:10:13 (string) len=5 "\"one\""
// map_literal_keys.baml:10:19 (operator) len=1 "|"
// map_literal_keys.baml:10:21 (string) len=5 "\"two\""
// map_literal_keys.baml:10:27 (operator) len=1 "|"
// map_literal_keys.baml:10:30 (string) len=7 "\"three\""
// map_literal_keys.baml:10:38 (operator) len=1 "|"
// map_literal_keys.baml:10:40 (string) len=6 "\"four\""
// map_literal_keys.baml:10:49 (type) [defaultLibrary] len=6 "string"
// map_literal_keys.baml:10:55 (operator) len=1 ">"
// map_literal_keys.baml:13:1 (keyword) len=8 "function"
// map_literal_keys.baml:13:10 (function) [declaration] len=12 "InOutEnumKey"
// map_literal_keys.baml:13:23 (parameter) [declaration] len=2 "i1"
// map_literal_keys.baml:13:27 (type) [defaultLibrary] len=3 "map"
// map_literal_keys.baml:13:30 (operator) len=1 "<"
// map_literal_keys.baml:13:31 (enum) len=6 "MapKey"
// map_literal_keys.baml:13:39 (type) [defaultLibrary] len=6 "string"
// map_literal_keys.baml:13:45 (operator) len=1 ">"
// map_literal_keys.baml:13:48 (parameter) [declaration] len=2 "i2"
// map_literal_keys.baml:13:52 (type) [defaultLibrary] len=3 "map"
// map_literal_keys.baml:13:55 (operator) len=1 "<"
// map_literal_keys.baml:13:56 (enum) len=6 "MapKey"
// map_literal_keys.baml:13:64 (type) [defaultLibrary] len=6 "string"
// map_literal_keys.baml:13:70 (operator) len=1 ">"
// map_literal_keys.baml:13:76 (type) [defaultLibrary] len=3 "map"
// map_literal_keys.baml:13:79 (operator) len=1 "<"
// map_literal_keys.baml:13:80 (enum) len=6 "MapKey"
// map_literal_keys.baml:13:88 (type) [defaultLibrary] len=6 "string"
// map_literal_keys.baml:13:94 (operator) len=1 ">"
// map_literal_keys.baml:14:5 (property) len=6 "client"
// map_literal_keys.baml:14:13 (string) len=15 "\"openai/gpt-4o\""
// map_literal_keys.baml:15:5 (property) [declaration] len=6 "prompt"
// map_literal_keys.baml:15:13 (string) len=1 "`"
// map_literal_keys.baml:16:9 (string) len=5 "Merge"
// map_literal_keys.baml:16:15 (string) len=5 "these"
// map_literal_keys.baml:16:20 (string) len=1 ":"
// map_literal_keys.baml:16:24 (parameter) len=2 "i1"
// map_literal_keys.baml:16:30 (parameter) len=2 "i2"
// map_literal_keys.baml:18:15 (property) len=13 "output_format"
// map_literal_keys.baml:19:5 (string) len=1 "`"
// map_literal_keys.baml:22:1 (keyword) len=8 "function"
// map_literal_keys.baml:22:10 (function) [declaration] len=29 "InOutLiteralStringUnionMapKey"
// map_literal_keys.baml:23:5 (parameter) [declaration] len=2 "i1"
// map_literal_keys.baml:23:9 (type) [defaultLibrary] len=3 "map"
// map_literal_keys.baml:23:12 (operator) len=1 "<"
// map_literal_keys.baml:23:13 (string) len=5 "\"one\""
// map_literal_keys.baml:23:19 (operator) len=1 "|"
// map_literal_keys.baml:23:21 (string) len=5 "\"two\""
// map_literal_keys.baml:23:27 (operator) len=1 "|"
// map_literal_keys.baml:23:30 (string) len=7 "\"three\""
// map_literal_keys.baml:23:38 (operator) len=1 "|"
// map_literal_keys.baml:23:40 (string) len=6 "\"four\""
// map_literal_keys.baml:23:49 (type) [defaultLibrary] len=6 "string"
// map_literal_keys.baml:23:55 (operator) len=1 ">"
// map_literal_keys.baml:24:5 (parameter) [declaration] len=2 "i2"
// map_literal_keys.baml:24:9 (type) [defaultLibrary] len=3 "map"
// map_literal_keys.baml:24:12 (operator) len=1 "<"
// map_literal_keys.baml:24:13 (string) len=5 "\"one\""
// map_literal_keys.baml:24:19 (operator) len=1 "|"
// map_literal_keys.baml:24:21 (string) len=5 "\"two\""
// map_literal_keys.baml:24:27 (operator) len=1 "|"
// map_literal_keys.baml:24:30 (string) len=7 "\"three\""
// map_literal_keys.baml:24:38 (operator) len=1 "|"
// map_literal_keys.baml:24:40 (string) len=6 "\"four\""
// map_literal_keys.baml:24:49 (type) [defaultLibrary] len=6 "string"
// map_literal_keys.baml:24:55 (operator) len=1 ">"
// map_literal_keys.baml:25:6 (type) [defaultLibrary] len=3 "map"
// map_literal_keys.baml:25:9 (operator) len=1 "<"
// map_literal_keys.baml:25:10 (string) len=5 "\"one\""
// map_literal_keys.baml:25:16 (operator) len=1 "|"
// map_literal_keys.baml:25:18 (string) len=5 "\"two\""
// map_literal_keys.baml:25:24 (operator) len=1 "|"
// map_literal_keys.baml:25:27 (string) len=7 "\"three\""
// map_literal_keys.baml:25:35 (operator) len=1 "|"
// map_literal_keys.baml:25:37 (string) len=6 "\"four\""
// map_literal_keys.baml:25:46 (type) [defaultLibrary] len=6 "string"
// map_literal_keys.baml:25:52 (operator) len=1 ">"
// map_literal_keys.baml:26:5 (property) len=6 "client"
// map_literal_keys.baml:26:13 (string) len=15 "\"openai/gpt-4o\""
// map_literal_keys.baml:27:5 (property) [declaration] len=6 "prompt"
// map_literal_keys.baml:27:13 (string) len=1 "`"
// map_literal_keys.baml:28:9 (string) len=5 "Merge"
// map_literal_keys.baml:28:15 (string) len=5 "these"
// map_literal_keys.baml:28:20 (string) len=1 ":"
// map_literal_keys.baml:30:11 (parameter) len=2 "i1"
// map_literal_keys.baml:32:11 (parameter) len=2 "i2"
// map_literal_keys.baml:34:15 (property) len=13 "output_format"
// map_literal_keys.baml:35:5 (string) len=1 "`"
