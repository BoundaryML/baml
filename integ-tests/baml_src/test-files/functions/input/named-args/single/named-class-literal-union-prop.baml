class LiteralClassOne {
  prop "one"
}

class LiteralClassTwo {
  prop "two"
}

function FnLiteralUnionClassInputOutput(input: LiteralClassOne | LiteralClassTwo) -> LiteralClassOne | LiteralClassTwo {
  client GPT4
  prompt #"
    Return the same object you were given.
    {{ ctx.output_format }}

    {{ _.role('user') }}
    {{ input }}
  "#
}

test TestFnLiteralUnionClassInputOutput{
  functions [FnLiteralUnionClassInputOutput]
  args {
    input {
      prop "one"
    }
  }
}