type RecursiveMapAlias = map<string, RecursiveMapAlias>

function FnOutputTypeAliasRecursiveMap(input: RecursiveMapAlias) -> RecursiveMapAlias {
    client "openai/gpt-4o"
    prompt #"
      Return the given value:

      {{ input }}

      {{ ctx.output_format }}
    "#
}

test TestOutputTypeAliasRecursiveMap {
  functions [FnOutputTypeAliasRecursiveMap]
  args {
    input {}
  }
}
