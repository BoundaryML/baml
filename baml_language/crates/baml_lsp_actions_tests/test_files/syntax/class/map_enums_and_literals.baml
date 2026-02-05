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
// <no-diagnostics-expected>
