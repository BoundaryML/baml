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
  client "openai/gpt-4o"
  prompt #"
    Merge these: {{i1}} {{i2}}

    {{ ctx.output_format }}
  "#
}

function InOutLiteralStringUnionMapKey(
  i1: map<"one" | "two" | ("three" | "four"), string>,
  i2: map<"one" | "two" | ("three" | "four"), string>
) -> map<"one" | "two" | ("three" | "four"), string> {
  client "openai/gpt-4o"
  prompt #"
    Merge these:

    {{i1}}

    {{i2}}

    {{ ctx.output_format }}
  "#
}

//----
//- diagnostics
// error: map keys must be `string`; got `MapKey`. Convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     ╭─[ class_map_enums_and_literals.baml:13:75 ]
//     │
//  13 │ function InOutEnumKey(i1: map<MapKey, string>, i2: map<MapKey, string>) -> map<MapKey, string> {
//     │                                                                           ──────────┬─────────
//     │                                                                                     ╰─────────── map keys must be `string`; got `MapKey`. Convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     │
//     │ Note: Error code: E0067
// ────╯
// error: map keys must be `string`; got `MapKey`. Convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     ╭─[ class_map_enums_and_literals.baml:13:26 ]
//     │
//  13 │ function InOutEnumKey(i1: map<MapKey, string>, i2: map<MapKey, string>) -> map<MapKey, string> {
//     │                          ──────────┬─────────
//     │                                    ╰─────────── map keys must be `string`; got `MapKey`. Convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     │
//     │ Note: Error code: E0067
// ────╯
// error: map keys must be `string`; got `MapKey`. Convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     ╭─[ class_map_enums_and_literals.baml:13:51 ]
//     │
//  13 │ function InOutEnumKey(i1: map<MapKey, string>, i2: map<MapKey, string>) -> map<MapKey, string> {
//     │                                                   ──────────┬─────────
//     │                                                             ╰─────────── map keys must be `string`; got `MapKey`. Convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     │
//     │ Note: Error code: E0067
// ────╯
// error: map keys must be `string`; got `MapKey`. Convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     ╭─[ class_map_enums_and_literals.baml:13:95 ]
//     │
//  13 │ ╭─▶ function InOutEnumKey(i1: map<MapKey, string>, i2: map<MapKey, string>) -> map<MapKey, string> {
//     ┆ ┆
//  20 │ ├─▶ }
//     │ │
//     │ ╰─────── map keys must be `string`; got `MapKey`. Convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     │
//     │     Note: Error code: E0067
// ────╯
// error: map keys must be `string`; got `MapKey`. Convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     ╭─[ class_map_enums_and_literals.baml:13:26 ]
//     │
//  13 │ function InOutEnumKey(i1: map<MapKey, string>, i2: map<MapKey, string>) -> map<MapKey, string> {
//     │                          ──────────┬─────────
//     │                                    ╰─────────── map keys must be `string`; got `MapKey`. Convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     │
//     │ Note: Error code: E0067
// ────╯
// error: map keys must be `string`; got `MapKey`. Convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     ╭─[ class_map_enums_and_literals.baml:13:51 ]
//     │
//  13 │ function InOutEnumKey(i1: map<MapKey, string>, i2: map<MapKey, string>) -> map<MapKey, string> {
//     │                                                   ──────────┬─────────
//     │                                                             ╰─────────── map keys must be `string`; got `MapKey`. Convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     │
//     │ Note: Error code: E0067
// ────╯
// error: map keys must be `string`; got `MapKey`. Convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     ╭─[ class_map_enums_and_literals.baml:13:26 ]
//     │
//  13 │ function InOutEnumKey(i1: map<MapKey, string>, i2: map<MapKey, string>) -> map<MapKey, string> {
//     │                          ──────────┬─────────
//     │                                    ╰─────────── map keys must be `string`; got `MapKey`. Convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     │
//     │ Note: Error code: E0067
// ────╯
// error: map keys must be `string`; got `MapKey`. Convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     ╭─[ class_map_enums_and_literals.baml:13:51 ]
//     │
//  13 │ function InOutEnumKey(i1: map<MapKey, string>, i2: map<MapKey, string>) -> map<MapKey, string> {
//     │                                                   ──────────┬─────────
//     │                                                             ╰─────────── map keys must be `string`; got `MapKey`. Convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     │
//     │ Note: Error code: E0067
// ────╯
// error: map keys must be `string`; got `MapKey`. Convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     ╭─[ class_map_enums_and_literals.baml:13:26 ]
//     │
//  13 │ function InOutEnumKey(i1: map<MapKey, string>, i2: map<MapKey, string>) -> map<MapKey, string> {
//     │                          ──────────┬─────────
//     │                                    ╰─────────── map keys must be `string`; got `MapKey`. Convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     │
//     │ Note: Error code: E0067
// ────╯
// error: map keys must be `string`; got `MapKey`. Convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     ╭─[ class_map_enums_and_literals.baml:13:51 ]
//     │
//  13 │ function InOutEnumKey(i1: map<MapKey, string>, i2: map<MapKey, string>) -> map<MapKey, string> {
//     │                                                   ──────────┬─────────
//     │                                                             ╰─────────── map keys must be `string`; got `MapKey`. Convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     │
//     │ Note: Error code: E0067
// ────╯
// error: map keys must be `string`; got `MapKey`. Convert non-string keys with `.to_string()` before `.set()` or `.get()`
//    ╭─[ class_map_enums_and_literals.baml:8:4 ]
//    │
//  8 │   e map<MapKey, string>
//    │    ──────────┬─────────
//    │              ╰─────────── map keys must be `string`; got `MapKey`. Convert non-string keys with `.to_string()` before `.set()` or `.get()`
//    │
//    │ Note: Error code: E0067
// ───╯
