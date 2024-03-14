function TestFnNamedArgsSingleBool {
  input (myBool: bool)
  output string
}

impl<llm, TestFnNamedArgsSingleBool> v1 {
  client GPT35
  prompt #"
    Return this value back to me: {#input.myBool}
  "#
}