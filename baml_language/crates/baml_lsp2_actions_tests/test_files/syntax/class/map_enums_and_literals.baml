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
// E0010
//
//   × Jinja `#"..."#` prompts are no longer supported. Use a backtick prompt with `${...}` interpolation instead.
//     ╭─[class_map_enums_and_literals.baml:15:10]
//  15 │ ╭─▶   prompt #"
//  16 │ │       Merge these: {{i1}} {{i2}}
//  17 │ │
//  18 │ │       {{ ctx.output_format }}
//  19 │ ├─▶   "#
//     · ╰──── use a backtick prompt instead
//     ╰────
// E0010
//
//   × Jinja `#"..."#` prompts are no longer supported. Use a backtick prompt with `${...}` interpolation instead.
//     ╭─[class_map_enums_and_literals.baml:27:10]
//  27 │ ╭─▶   prompt #"
//  28 │ │       Merge these:
//  29 │ │
//  30 │ │       {{i1}}
//  31 │ │
//  32 │ │       {{i2}}
//  33 │ │
//  34 │ │       {{ ctx.output_format }}
//  35 │ ├─▶   "#
//     · ╰──── use a backtick prompt instead
//     ╰────
// E0067
//
//   × map keys must be `string`; got `MapKey`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     ╭─[class_map_enums_and_literals.baml:13:27]
//  13 │ function InOutEnumKey(i1: map<MapKey, string>, i2: map<MapKey, string>) -> map<MapKey, string> {
//     ·                           ───────────────────
//     ╰────
// E0067
//
//   × map keys must be `string`; got `MapKey`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     ╭─[class_map_enums_and_literals.baml:13:52]
//  13 │ function InOutEnumKey(i1: map<MapKey, string>, i2: map<MapKey, string>) -> map<MapKey, string> {
//     ·                                                    ───────────────────
//     ╰────
// E0067
//
//   × map keys must be `string`; got `MapKey`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     ╭─[class_map_enums_and_literals.baml:13:76]
//  13 │ function InOutEnumKey(i1: map<MapKey, string>, i2: map<MapKey, string>) -> map<MapKey, string> {
//     ·                                                                            ───────────────────
//     ╰────
// E0067
//
//   × map keys must be `string`; got `MapKey`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//    ╭─[class_map_enums_and_literals.baml:8:5]
//  8 │   e map<MapKey, string>
//    ·     ───────────────────
//    ╰────
