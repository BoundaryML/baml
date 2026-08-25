enum MapKey {
  A
  B
  C
}

class Fields {
  e map<MapKey, string>
  l1 map<"literal", string>
  l2 map<"one" | "two" | ("three" | "four"), string>
}

function InOutEnumKey(i1: map<MapKey, string>, i2: map<MapKey, string>) -> map<MapKey, string> {
  client: "openai/gpt-4o"
  prompt: `
    Merge these: ${i1} ${i2}

    ${ctx.output_format()}
  `
}

function InOutLiteralStringUnionMapKey(
  i1: map<"one" | "two" | ("three" | "four"), string>,
  i2: map<"one" | "two" | ("three" | "four"), string>
) -> map<"one" | "two" | ("three" | "four"), string> {
  client: "openai/gpt-4o"
  prompt: `
    Merge these:

    ${i1}

    ${i2}

    ${ctx.output_format()}
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
//    ╭─[map_literal_keys.baml:1:1]
//  1 │ enum MapKey {
//    · ▲
//    ╰────
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
//    ╭─[map_literal_keys.baml:1:1]
//  1 │ enum MapKey {
//    · ▲
//    ╰────
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
//    ╭─[map_literal_keys.baml:8:5]
//  8 │   e map<MapKey, string>
//    ·     ───────────────────
//    ╰────
//
//- semantic_tokens
// map_literal_keys.baml:1:1 (keyword) len=4 "enum"
// map_literal_keys.baml:1:6 (enum) [declaration] len=6 "MapKey"
// map_literal_keys.baml:2:3 (enumMember) [declaration] len=1 "A"
// map_literal_keys.baml:3:3 (enumMember) [declaration] len=1 "B"
// map_literal_keys.baml:4:3 (enumMember) [declaration] len=1 "C"
// map_literal_keys.baml:7:1 (keyword) len=5 "class"
// map_literal_keys.baml:7:7 (class) [declaration] len=6 "Fields"
// map_literal_keys.baml:8:3 (property) [declaration] len=1 "e"
// map_literal_keys.baml:8:5 (type) [defaultLibrary] len=3 "map"
// map_literal_keys.baml:8:8 (operator) len=1 "<"
// map_literal_keys.baml:8:9 (enum) len=6 "MapKey"
// map_literal_keys.baml:8:17 (type) [defaultLibrary] len=6 "string"
// map_literal_keys.baml:8:23 (operator) len=1 ">"
// map_literal_keys.baml:9:3 (property) [declaration] len=2 "l1"
// map_literal_keys.baml:9:6 (type) [defaultLibrary] len=3 "map"
// map_literal_keys.baml:9:9 (operator) len=1 "<"
// map_literal_keys.baml:9:10 (string) len=9 "\"literal\""
// map_literal_keys.baml:9:21 (type) [defaultLibrary] len=6 "string"
// map_literal_keys.baml:9:27 (operator) len=1 ">"
// map_literal_keys.baml:10:3 (property) [declaration] len=2 "l2"
// map_literal_keys.baml:10:6 (type) [defaultLibrary] len=3 "map"
// map_literal_keys.baml:10:9 (operator) len=1 "<"
// map_literal_keys.baml:10:10 (string) len=5 "\"one\""
// map_literal_keys.baml:10:16 (operator) len=1 "|"
// map_literal_keys.baml:10:18 (string) len=5 "\"two\""
// map_literal_keys.baml:10:24 (operator) len=1 "|"
// map_literal_keys.baml:10:27 (string) len=7 "\"three\""
// map_literal_keys.baml:10:35 (operator) len=1 "|"
// map_literal_keys.baml:10:37 (string) len=6 "\"four\""
// map_literal_keys.baml:10:46 (type) [defaultLibrary] len=6 "string"
// map_literal_keys.baml:10:52 (operator) len=1 ">"
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
// map_literal_keys.baml:14:3 (property) len=6 "client"
// map_literal_keys.baml:14:11 (string) len=15 "\"openai/gpt-4o\""
// map_literal_keys.baml:15:3 (property) [declaration] len=6 "prompt"
// map_literal_keys.baml:15:11 (string) len=1 "`"
// map_literal_keys.baml:16:5 (string) len=5 "Merge"
// map_literal_keys.baml:16:11 (string) len=5 "these"
// map_literal_keys.baml:16:16 (string) len=1 ":"
// map_literal_keys.baml:16:20 (parameter) len=2 "i1"
// map_literal_keys.baml:16:26 (parameter) len=2 "i2"
// map_literal_keys.baml:18:11 (method) len=13 "output_format"
// map_literal_keys.baml:19:3 (string) len=1 "`"
// map_literal_keys.baml:22:1 (keyword) len=8 "function"
// map_literal_keys.baml:22:10 (function) [declaration] len=29 "InOutLiteralStringUnionMapKey"
// map_literal_keys.baml:23:3 (parameter) [declaration] len=2 "i1"
// map_literal_keys.baml:23:7 (type) [defaultLibrary] len=3 "map"
// map_literal_keys.baml:23:10 (operator) len=1 "<"
// map_literal_keys.baml:23:11 (string) len=5 "\"one\""
// map_literal_keys.baml:23:17 (operator) len=1 "|"
// map_literal_keys.baml:23:19 (string) len=5 "\"two\""
// map_literal_keys.baml:23:25 (operator) len=1 "|"
// map_literal_keys.baml:23:28 (string) len=7 "\"three\""
// map_literal_keys.baml:23:36 (operator) len=1 "|"
// map_literal_keys.baml:23:38 (string) len=6 "\"four\""
// map_literal_keys.baml:23:47 (type) [defaultLibrary] len=6 "string"
// map_literal_keys.baml:23:53 (operator) len=1 ">"
// map_literal_keys.baml:24:3 (parameter) [declaration] len=2 "i2"
// map_literal_keys.baml:24:7 (type) [defaultLibrary] len=3 "map"
// map_literal_keys.baml:24:10 (operator) len=1 "<"
// map_literal_keys.baml:24:11 (string) len=5 "\"one\""
// map_literal_keys.baml:24:17 (operator) len=1 "|"
// map_literal_keys.baml:24:19 (string) len=5 "\"two\""
// map_literal_keys.baml:24:25 (operator) len=1 "|"
// map_literal_keys.baml:24:28 (string) len=7 "\"three\""
// map_literal_keys.baml:24:36 (operator) len=1 "|"
// map_literal_keys.baml:24:38 (string) len=6 "\"four\""
// map_literal_keys.baml:24:47 (type) [defaultLibrary] len=6 "string"
// map_literal_keys.baml:24:53 (operator) len=1 ">"
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
// map_literal_keys.baml:26:3 (property) len=6 "client"
// map_literal_keys.baml:26:11 (string) len=15 "\"openai/gpt-4o\""
// map_literal_keys.baml:27:3 (property) [declaration] len=6 "prompt"
// map_literal_keys.baml:27:11 (string) len=1 "`"
// map_literal_keys.baml:28:5 (string) len=5 "Merge"
// map_literal_keys.baml:28:11 (string) len=5 "these"
// map_literal_keys.baml:28:16 (string) len=1 ":"
// map_literal_keys.baml:30:7 (parameter) len=2 "i1"
// map_literal_keys.baml:32:7 (parameter) len=2 "i2"
// map_literal_keys.baml:34:11 (method) len=13 "output_format"
// map_literal_keys.baml:35:3 (string) len=1 "`"
