type RecursiveListAlias = RecursiveListAlias[]

function FnOutputTypeAliasRecursiveList(input: RecursiveListAlias) -> RecursiveListAlias {
    client "openai/gpt-4o"
    prompt #"
      Return the given JSON array:

      {{ input }}

      {{ ctx.output_format }}
    "#
}

test TestOutputTypeAliasRecursiveList {
  functions [FnOutputTypeAliasRecursiveList]
  args {
    input []
  }
}
