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

  d1 map<>
  d2 map<string>
  d3 map<string, string, string>
}

function InputAndOutput(i1: map<string, string>, i2: map<MapDummy, string>) -> map<string, int> {
  client MyClient
  prompt #"
    Merge these: {{i1}} {{i2}}
  "#
}

// error: Error validating: This line is not a valid field or attribute definition. A valid class property looks like: 'myProperty string[] @description("This is a description")'
//   -->  class/map_types2.baml:26
//    |
// 25 |
// 26 |   d1 map<>
// 27 |   d2 map<string>
//    |
// error: Error validating: This line is not a valid field or attribute definition. A valid class property looks like: 'myProperty string[] @description("This is a description")'
//   -->  class/map_types2.baml:27
//    |
// 26 |   d1 map<>
// 27 |   d2 map<string>
// 28 |   d3 map<string, string, string>
//    |
// error: Error validating: This line is not a valid field or attribute definition. A valid class property looks like: 'myProperty string[] @description("This is a description")'
//   -->  class/map_types2.baml:28
//    |
// 27 |   d2 map<string>
// 28 |   d3 map<string, string, string>
// 29 | }
//    |

//----
//- diagnostics
// error: Expected type, found '>'
//     ╭─[ class_map_types2.baml:26:10 ]
//     │
//  26 │   d1 map<>
//     │          ┬
//     │          ╰── Expected type, found '>'
//     │
//     │ Note: Error code: E0010
// ────╯
// error: map keys must be `string`; got `MapDummy`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     ╭─[ class_map_types2.baml:31:53 ]
//     │
//  31 │ function InputAndOutput(i1: map<string, string>, i2: map<MapDummy, string>) -> map<string, int> {
//     │                                                     ───────────┬──────────
//     │                                                                ╰──────────── map keys must be `string`; got `MapDummy`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     │
//     │ Note: Error code: E0067
// ────╯
// error: map keys must be `string`; got `int`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     ╭─[ class_map_types2.baml:16:5 ]
//     │
//  16 │   b1 map<int, string>
//     │     ────────┬────────
//     │             ╰────────── map keys must be `string`; got `int`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     │
//     │ Note: Error code: E0067
// ────╯
// error: map keys must be `string`; got `float`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     ╭─[ class_map_types2.baml:17:5 ]
//     │
//  17 │   b2 map<float, string>
//     │     ─────────┬─────────
//     │              ╰─────────── map keys must be `string`; got `float`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     │
//     │ Note: Error code: E0067
// ────╯
// error: map keys must be `string`; got `MapDummy`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     ╭─[ class_map_types2.baml:18:5 ]
//     │
//  18 │   b3 map<MapDummy, string>
//     │     ───────────┬──────────
//     │                ╰──────────── map keys must be `string`; got `MapDummy`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     │
//     │ Note: Error code: E0067
// ────╯
// error: map keys must be `string`; got `string | null`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     ╭─[ class_map_types2.baml:19:5 ]
//     │
//  19 │   b4 map<string?, string>
//     │     ──────────┬──────────
//     │               ╰──────────── map keys must be `string`; got `string | null`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     │
//     │ Note: Error code: E0067
// ────╯
// error: map keys must be `string`; got `string | int`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     ╭─[ class_map_types2.baml:20:5 ]
//     │
//  20 │   b5 map<string | int, string>
//     │     ─────────────┬────────────
//     │                  ╰────────────── map keys must be `string`; got `string | int`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     │
//     │ Note: Error code: E0067
// ────╯
// error: map keys must be `string`; got `int`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     ╭─[ class_map_types2.baml:23:5 ]
//     │
//  23 │   c2 string | map<int, string>
//     │     ─────────────┬────────────
//     │                  ╰────────────── map keys must be `string`; got `int`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     │
//     │ Note: Error code: E0067
// ────╯
// error: map keys must be `string`; got `string | null`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     ╭─[ class_map_types2.baml:24:5 ]
//     │
//  24 │   c3 string | map<string?, string>
//     │     ───────────────┬──────────────
//     │                    ╰──────────────── map keys must be `string`; got `string | null`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     │
//     │ Note: Error code: E0067
// ────╯
// error: unresolved type: map
//     ╭─[ class_map_types2.baml:26:5 ]
//     │
//  26 │   d1 map<>
//     │     ───┬──
//     │        ╰──── unresolved type: map
//     │
//     │ Note: Error code: E0002
// ────╯
// error: unresolved type: map
//     ╭─[ class_map_types2.baml:27:5 ]
//     │
//  27 │   d2 map<string>
//     │     ──────┬─────
//     │           ╰─────── unresolved type: map
//     │
//     │ Note: Error code: E0002
// ────╯
// error: unresolved type: map
//     ╭─[ class_map_types2.baml:28:5 ]
//     │
//  28 │   d3 map<string, string, string>
//     │     ──────────────┬─────────────
//     │                   ╰─────────────── unresolved type: map
//     │
//     │ Note: Error code: E0002
// ────╯
