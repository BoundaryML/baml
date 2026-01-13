type RecAliasOne = RecAliasTwo
type RecAliasTwo = RecAliasThree
type RecAliasThree = RecAliasOne[]

function FnOutputTypeAliasRecursiveCycle(input: RecAliasOne) -> RecAliasOne {
    client "openai/gpt-4o"
    prompt #"
      Return the given JSON array:

      {{ input }}

      {{ ctx.output_format }}
    "#
}

test TestOutputTypeAliasRecursiveCycle {
  functions [FnOutputTypeAliasRecursiveCycle]
  args {
    input []
  }
}
