function FnOutputBool {
  input string
  output bool
}

impl<llm, FnOutputBool> v1 {
  client GPT35
  prompt #"
    Return a {#print_type(output)}:
  "#
}

