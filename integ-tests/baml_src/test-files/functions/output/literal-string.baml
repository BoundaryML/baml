function FnOutputLiteralString(input: string) -> "example output" {
  client GPT35
  prompt #"
    Return a string: {{ ctx.output_format}}
  "#
}

function FnOutputLiteralStringExprFunc(input: string) -> "example output" {
  return "example output";
}

test FnOutputLiteralString {
  functions [FnOutputLiteralString]
  args {
    input "example input"
  }
}
