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
// Error: Invalid type MapKey for map key. Only strings, string literals and enums are valid map keys.
//     ╭─[ class_map_enums_and_literals.baml:13:75 ]
//     │
//  13 │ function InOutEnumKey(i1: map<MapKey, string>, i2: map<MapKey, string>) -> map<MapKey, string> {
//     │                                                                           ──────────┬─────────  
//     │                                                                                     ╰─────────── Invalid type MapKey for map key. Only strings, string literals and enums are valid map keys.
//     │ 
//     │ Note: Error code: E0067
// ────╯
// Error: Invalid type MapKey for map key. Only strings, string literals and enums are valid map keys.
//     ╭─[ class_map_enums_and_literals.baml:13:23 ]
//     │
//  13 │ function InOutEnumKey(i1: map<MapKey, string>, i2: map<MapKey, string>) -> map<MapKey, string> {
//     │                       ───────────┬───────────  
//     │                                  ╰───────────── Invalid type MapKey for map key. Only strings, string literals and enums are valid map keys.
//     │ 
//     │ Note: Error code: E0067
// ────╯
// Error: Invalid type MapKey for map key. Only strings, string literals and enums are valid map keys.
//     ╭─[ class_map_enums_and_literals.baml:13:47 ]
//     │
//  13 │ function InOutEnumKey(i1: map<MapKey, string>, i2: map<MapKey, string>) -> map<MapKey, string> {
//     │                                               ────────────┬───────────  
//     │                                                           ╰───────────── Invalid type MapKey for map key. Only strings, string literals and enums are valid map keys.
//     │ 
//     │ Note: Error code: E0067
// ────╯
