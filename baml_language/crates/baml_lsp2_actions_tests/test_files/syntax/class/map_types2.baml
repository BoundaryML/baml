// Tests for validating map types

client MyClient = openai.OpenAiClient.new(model = "gpt-4o");

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
  client: MyClient
  prompt: `
    Merge these: ${i1} ${i2}
  `
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
//     ╭─[class_map_types2.baml:24:10]
//  24 │   d1 map<>
//     ·          ┬
//     ·          ╰── expected `type`, found `'>'`
//     ╰────
// E0067
//
//   × map keys must be `string`; got `MapDummy`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     ╭─[class_map_types2.baml:29:54]
//  29 │ function InputAndOutput(i1: map<string, string>, i2: map<MapDummy, string>) -> map<string, int> {
//     ·                                                      ─────────────────────
//     ╰────
// E0067
//
//   × map keys must be `string`; got `int`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     ╭─[class_map_types2.baml:14:6]
//  14 │   b1 map<int, string>
//     ·      ────────────────
//     ╰────
// E0067
//
//   × map keys must be `string`; got `float`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     ╭─[class_map_types2.baml:15:6]
//  15 │   b2 map<float, string>
//     ·      ──────────────────
//     ╰────
// E0067
//
//   × map keys must be `string`; got `MapDummy`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     ╭─[class_map_types2.baml:16:6]
//  16 │   b3 map<MapDummy, string>
//     ·      ─────────────────────
//     ╰────
// E0067
//
//   × map keys must be `string`; got `string | null`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     ╭─[class_map_types2.baml:17:6]
//  17 │   b4 map<string?, string>
//     ·      ────────────────────
//     ╰────
// E0067
//
//   × map keys must be `string`; got `string | int`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     ╭─[class_map_types2.baml:18:6]
//  18 │   b5 map<string | int, string>
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
//     ╭─[class_map_types2.baml:14:6]
//  14 │   b1 map<int, string>
//     ·      ────────────────
//     ╰────
// E0067
//
//   × map keys must be `string`; got `float`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     ╭─[class_map_types2.baml:15:6]
//  15 │   b2 map<float, string>
//     ·      ──────────────────
//     ╰────
// E0067
//
//   × map keys must be `string`; got `MapDummy`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     ╭─[class_map_types2.baml:16:6]
//  16 │   b3 map<MapDummy, string>
//     ·      ─────────────────────
//     ╰────
// E0067
//
//   × map keys must be `string`; got `string | null`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     ╭─[class_map_types2.baml:17:6]
//  17 │   b4 map<string?, string>
//     ·      ────────────────────
//     ╰────
// E0067
//
//   × map keys must be `string`; got `string | int`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     ╭─[class_map_types2.baml:18:6]
//  18 │   b5 map<string | int, string>
//     ·      ─────────────────────────
//     ╰────
// E0067
//
//   × map keys must be `string`; got `int`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     ╭─[class_map_types2.baml:21:15]
//  21 │   c2 string | map<int, string>
//     ·               ───
//     ╰────
// E0067
//
//   × map keys must be `string`; got `string | null`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     ╭─[class_map_types2.baml:22:15]
//  22 │   c3 string | map<string?, string>
//     ·               ───
//     ╰────
// E0002
//
//   × unresolved type: map
//     ╭─[class_map_types2.baml:24:6]
//  24 │   d1 map<>
//     ·      ─────
//     ╰────
// E0002
//
//   × unresolved type: map
//     ╭─[class_map_types2.baml:25:6]
//  25 │   d2 map<string>
//     ·      ───────────
//     ╰────
// E0002
//
//   × unresolved type: map
//     ╭─[class_map_types2.baml:26:6]
//  26 │   d3 map<string, string, string>
//     ·      ───────────────────────────
//     ╰────
