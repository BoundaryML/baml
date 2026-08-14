// Tests for validating map types

client MyClient = openai.ResponsesClient.new(model = "gpt-4o");

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
  prompt `
    Merge these: ${i1} ${i2}
  `
}

//----
//- diagnostics
// E0067
//
//   × map keys must be `string`; got `MapDummy`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     ╭─[class_map_types.baml:25:54]
//  25 │ function InputAndOutput(i1: map<string, string>, i2: map<MapDummy, string>) -> map<string, int> {
//     ·                                                      ─────────────────────
//     ╰────
// E0067
//
//   × map keys must be `string`; got `int`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     ╭─[class_map_types.baml:14:6]
//  14 │   b1 map<int, string>
//     ·      ────────────────
//     ╰────
// E0067
//
//   × map keys must be `string`; got `float`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     ╭─[class_map_types.baml:15:6]
//  15 │   b2 map<float, string>
//     ·      ──────────────────
//     ╰────
// E0067
//
//   × map keys must be `string`; got `MapDummy`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     ╭─[class_map_types.baml:16:6]
//  16 │   b3 map<MapDummy, string>
//     ·      ─────────────────────
//     ╰────
// E0067
//
//   × map keys must be `string`; got `string | null`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     ╭─[class_map_types.baml:17:6]
//  17 │   b4 map<string?, string>
//     ·      ────────────────────
//     ╰────
// E0067
//
//   × map keys must be `string`; got `string | int`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     ╭─[class_map_types.baml:18:6]
//  18 │   b5 map<string | int, string>
//     ·      ─────────────────────────
//     ╰────
// E0067
//
//   × map keys must be `string`; got `int`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     ╭─[class_map_types.baml:21:6]
//  21 │   c2 string | map<int, string>
//     ·      ─────────────────────────
//     ╰────
// E0067
//
//   × map keys must be `string`; got `string | null`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     ╭─[class_map_types.baml:22:6]
//  22 │   c3 string | map<string?, string>
//     ·      ─────────────────────────────
//     ╰────
