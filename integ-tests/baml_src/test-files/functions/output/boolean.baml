function FnOutputBool(input: string) -> bool {
  client GPT35
  prompt #"
    Return "true"
  "#
}

function FnOutputBoolExprFunc(input: string) -> bool {
  return true;
}

test FnOutputBool {
  functions [FnOutputBool]
  args {
    input "example input"
  }
}
