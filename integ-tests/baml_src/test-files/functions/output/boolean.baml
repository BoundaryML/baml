function FnOutputBool(input: string) -> bool {
  client GPT35
  prompt #"
    Return a true: {{ ctx.output_format}}:
  "#
}

test FnOutputBool {
  functions [FnOutputBool]
  args {
    input "example input"
  }
}
