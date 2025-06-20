class LiteralClassHello {
  prop "hello"
}

function FnLiteralClassInputOutput(input: LiteralClassHello) -> LiteralClassHello {
  client GPT4
  prompt #"
    Return the same object you were given.
    {{ ctx.output_format }}
  "#
}

test TestFnLiteralClassInputOutput{
  functions [FnLiteralClassInputOutput]
  args {
    input {
      prop "hello"
    }
  }
}