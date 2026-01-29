function FnOutputLiteralBool(input: string) -> false {
  client GPT35
  prompt #"
    Return a false: {{ ctx.output_format}}
  "#
}

// TODO:
// function FnOutputLiteralBoolExprFunc(input: string) -> false {
//   return false;
// }

test FnOutputLiteralBool {
  functions [FnOutputLiteralBool]
  args {
    input "example input"
  }
}
