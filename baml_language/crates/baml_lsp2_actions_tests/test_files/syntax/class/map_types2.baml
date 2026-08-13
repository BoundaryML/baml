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
// E0010
//
//   × unexpected token
//     ╭─[class_map_types2.baml:26:10]
//  26 │   d1 map<>
//     ·          ┬
//     ·          ╰── expected `type`, found `'>'`
//     ╰────
// E0010
//
//   × `client<llm>` config blocks are removed; declare a client value instead: `client MyClient = openai.OpenAiClient.new(model = "...");` (compose reliability with ai.Retry / ai.Fallback /
//   │ ai.RoundRobin)
//    ╭─[class_map_types2.baml:3:1]
//  3 │ ╭─▶ client<llm> MyClient {
//  4 │ │     provider openai
//  5 │ ├─▶ }
//    · ╰──── replace with `client Name = <expr>;`
//    ╰────
// E0010
//
//   × Jinja `#"..."#` prompts are no longer supported. Use a backtick prompt with `${...}` interpolation instead.
//     ╭─[class_map_types2.baml:33:10]
//  33 │ ╭─▶   prompt #"
//  34 │ │       Merge these: {{i1}} {{i2}}
//  35 │ ├─▶   "#
//     · ╰──── use a backtick prompt instead
//     ╰────
// E0067
//
//   × map keys must be `string`; got `MapDummy`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     ╭─[class_map_types2.baml:31:54]
//  31 │ function InputAndOutput(i1: map<string, string>, i2: map<MapDummy, string>) -> map<string, int> {
//     ·                                                      ─────────────────────
//     ╰────
// E0067
//
//   × map keys must be `string`; got `int`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     ╭─[class_map_types2.baml:16:6]
//  16 │   b1 map<int, string>
//     ·      ────────────────
//     ╰────
// E0067
//
//   × map keys must be `string`; got `float`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     ╭─[class_map_types2.baml:17:6]
//  17 │   b2 map<float, string>
//     ·      ──────────────────
//     ╰────
// E0067
//
//   × map keys must be `string`; got `MapDummy`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     ╭─[class_map_types2.baml:18:6]
//  18 │   b3 map<MapDummy, string>
//     ·      ─────────────────────
//     ╰────
// E0067
//
//   × map keys must be `string`; got `string | null`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     ╭─[class_map_types2.baml:19:6]
//  19 │   b4 map<string?, string>
//     ·      ────────────────────
//     ╰────
// E0067
//
//   × map keys must be `string`; got `string | int`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     ╭─[class_map_types2.baml:20:6]
//  20 │   b5 map<string | int, string>
//     ·      ─────────────────────────
//     ╰────
// E0067
//
//   × map keys must be `string`; got `int`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//    ╭─[class_map_types2.baml:1:1]
//  1 │ // Tests for validating map types
//    · ▲
//    ╰────
// E0067
//
//   × map keys must be `string`; got `string | null`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//    ╭─[class_map_types2.baml:1:1]
//  1 │ // Tests for validating map types
//    · ▲
//    ╰────
// E0002
//
//   × unresolved type: map
//    ╭─[class_map_types2.baml:1:1]
//  1 │ // Tests for validating map types
//    · ▲
//    ╰────
// E0067
//
//   × map keys must be `string`; got `int`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     ╭─[class_map_types2.baml:16:6]
//  16 │   b1 map<int, string>
//     ·      ────────────────
//     ╰────
// E0067
//
//   × map keys must be `string`; got `float`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     ╭─[class_map_types2.baml:17:6]
//  17 │   b2 map<float, string>
//     ·      ──────────────────
//     ╰────
// E0067
//
//   × map keys must be `string`; got `MapDummy`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     ╭─[class_map_types2.baml:18:6]
//  18 │   b3 map<MapDummy, string>
//     ·      ─────────────────────
//     ╰────
// E0067
//
//   × map keys must be `string`; got `string | null`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     ╭─[class_map_types2.baml:19:6]
//  19 │   b4 map<string?, string>
//     ·      ────────────────────
//     ╰────
// E0067
//
//   × map keys must be `string`; got `string | int`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     ╭─[class_map_types2.baml:20:6]
//  20 │   b5 map<string | int, string>
//     ·      ─────────────────────────
//     ╰────
// E0067
//
//   × map keys must be `string`; got `int`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     ╭─[class_map_types2.baml:23:15]
//  23 │   c2 string | map<int, string>
//     ·               ───
//     ╰────
// E0067
//
//   × map keys must be `string`; got `string | null`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     ╭─[class_map_types2.baml:24:15]
//  24 │   c3 string | map<string?, string>
//     ·               ───
//     ╰────
// E0002
//
//   × unresolved type: map
//     ╭─[class_map_types2.baml:26:6]
//  26 │   d1 map<>
//     ·      ─────
//     ╰────
// E0002
//
//   × unresolved type: map
//     ╭─[class_map_types2.baml:27:6]
//  27 │   d2 map<string>
//     ·      ───────────
//     ╰────
// E0002
//
//   × unresolved type: map
//     ╭─[class_map_types2.baml:28:6]
//  28 │   d3 map<string, string, string>
//     ·      ───────────────────────────
//     ╰────
