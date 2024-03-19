function FnStringOptional {
  // TODO: playground wont accept a null string input. Must fix
  input string?
  output string
}

impl<llm, FnStringOptional> v1 {
  client GPT35
  prompt #"
    Return {#input}:
  "#
}