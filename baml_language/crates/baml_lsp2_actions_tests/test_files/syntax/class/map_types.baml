// Tests for validating map types

client<llm> MyClient {
  provider openai
}

class MapDummy {
  inner string
}

class MapFields {
  a1 map<string, string>
  a2 map<string, int>
  a3 map<string, MapDummy>

  b1 map<int, string>
  b2 map<float, string>
  b3 map<MapDummy, string>
  b4 map<string?, string>
  b5 map<string | int, string>

  c1 string | map<string, string>
  c2 string | map<int, string>
  c3 string | map<string?, string>
}

function InputAndOutput(i1: map<string, string>, i2: map<MapDummy, string>) -> map<string, int> {
  client MyClient
  prompt #"
    Merge these: {{i1}} {{i2}}
  "#
}

// error: Error validating: Maps may only have strings, enums or literal strings as keys
//   -->  class/map_types.baml:16
//    |
// 15 |
// 16 |   b1 map<int, string>
//    |
// error: Error validating: Maps may only have strings, enums or literal strings as keys
//   -->  class/map_types.baml:17
//    |
// 16 |   b1 map<int, string>
// 17 |   b2 map<float, string>
//    |
// error: Error validating: Maps may only have strings, enums or literal strings as keys
//   -->  class/map_types.baml:18
//    |
// 17 |   b2 map<float, string>
// 18 |   b3 map<MapDummy, string>
//    |
// error: Error validating: Maps may only have strings, enums or literal strings as keys
//   -->  class/map_types.baml:19
//    |
// 18 |   b3 map<MapDummy, string>
// 19 |   b4 map<string?, string>
//    |
// error: Error validating: Maps may only have strings, enums or literal strings as keys
//   -->  class/map_types.baml:20
//    |
// 19 |   b4 map<string?, string>
// 20 |   b5 map<string | int, string>
//    |
// error: Error validating: Maps may only have strings, enums or literal strings as keys
//   -->  class/map_types.baml:20
//    |
// 19 |   b4 map<string?, string>
// 20 |   b5 map<string | int, string>
//    |
// error: Error validating: Maps may only have strings, enums or literal strings as keys
//   -->  class/map_types.baml:23
//    |
// 22 |   c1 string | map<string, string>
// 23 |   c2 string | map<int, string>
//    |
// error: Error validating: Maps may only have strings, enums or literal strings as keys
//   -->  class/map_types.baml:24
//    |
// 23 |   c2 string | map<int, string>
// 24 |   c3 string | map<string?, string>
//    |
// error: Error validating: Maps may only have strings, enums or literal strings as keys
//   -->  class/map_types.baml:27
//    |
// 26 |
// 27 | function InputAndOutput(i1: map<string, string>, i2: map<MapDummy, string>) -> map<string, int> {
//    |

//----
//- diagnostics
// error: map keys must be `string`; got `MapDummy`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     ╭─[ class_map_types.baml:27:53 ]
//     │
//  27 │ function InputAndOutput(i1: map<string, string>, i2: map<MapDummy, string>) -> map<string, int> {
//     │                                                     ───────────┬──────────
//     │                                                                ╰──────────── map keys must be `string`; got `MapDummy`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     │
//     │ Note: Error code: E0067
// ────╯
// error: map keys must be `string`; got `int`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     ╭─[ class_map_types.baml:16:5 ]
//     │
//  16 │   b1 map<int, string>
//     │     ────────┬────────
//     │             ╰────────── map keys must be `string`; got `int`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     │
//     │ Note: Error code: E0067
// ────╯
// error: map keys must be `string`; got `float`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     ╭─[ class_map_types.baml:17:5 ]
//     │
//  17 │   b2 map<float, string>
//     │     ─────────┬─────────
//     │              ╰─────────── map keys must be `string`; got `float`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     │
//     │ Note: Error code: E0067
// ────╯
// error: map keys must be `string`; got `MapDummy`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     ╭─[ class_map_types.baml:18:5 ]
//     │
//  18 │   b3 map<MapDummy, string>
//     │     ───────────┬──────────
//     │                ╰──────────── map keys must be `string`; got `MapDummy`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     │
//     │ Note: Error code: E0067
// ────╯
// error: map keys must be `string`; got `string | null`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     ╭─[ class_map_types.baml:19:5 ]
//     │
//  19 │   b4 map<string?, string>
//     │     ──────────┬──────────
//     │               ╰──────────── map keys must be `string`; got `string | null`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     │
//     │ Note: Error code: E0067
// ────╯
// error: map keys must be `string`; got `string | int`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     ╭─[ class_map_types.baml:20:5 ]
//     │
//  20 │   b5 map<string | int, string>
//     │     ─────────────┬────────────
//     │                  ╰────────────── map keys must be `string`; got `string | int`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     │
//     │ Note: Error code: E0067
// ────╯
// error: map keys must be `string`; got `int`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     ╭─[ class_map_types.baml:23:5 ]
//     │
//  23 │   c2 string | map<int, string>
//     │     ─────────────┬────────────
//     │                  ╰────────────── map keys must be `string`; got `int`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     │
//     │ Note: Error code: E0067
// ────╯
// error: map keys must be `string`; got `string | null`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     ╭─[ class_map_types.baml:24:5 ]
//     │
//  24 │   c3 string | map<string?, string>
//     │     ───────────────┬──────────────
//     │                    ╰──────────────── map keys must be `string`; got `string | null`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     │
//     │ Note: Error code: E0067
// ────╯
