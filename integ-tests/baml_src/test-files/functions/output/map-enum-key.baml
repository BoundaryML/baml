enum MapKey {
  A
  B
  C
}

function InOutEnumMapKey(i1: map<MapKey, string>, i2: map<MapKey, string>) -> map<MapKey, string> {
  client "openai/gpt-4o"
  prompt #"
    Merge these: {{i1}} {{i2}}

    {{ ctx.output_format }}
  "#
}

// TODO: expr func
// function InOutEnumMapKeyExprFunc(i1: map<MapKey, string>, i2: map<MapKey, string>) -> map<MapKey, string> {
//   return i1;
// }