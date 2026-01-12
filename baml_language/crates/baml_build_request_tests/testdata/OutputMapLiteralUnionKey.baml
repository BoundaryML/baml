function FnOutputMapLiteralUnionKey(
  i1: map<"one" | "two" | ("three" | "four"), string>,
  i2: map<"one" | "two" | ("three" | "four"), string>
) -> map<"one" | "two" | ("three" | "four"), string> {
  client "openai/gpt-4o"
  prompt #"
    Merge these:

    {{i1}}

    {{i2}}

    {{ ctx.output_format() }}
  "#
}

function FnOutputMapLiteralUnionKey2(m: map<"key", string>) -> map<"key", string> {
  client "openai/gpt-4o"
  prompt #"
    Return the same map you were given:

    {{m}}

    {{ ctx.output_format() }}
  "#
}

test TestOutputMapLiteralUnionKey {
  functions [FnOutputMapLiteralUnionKey, FnOutputMapLiteralUnionKey2]
}
