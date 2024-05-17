function TestFnNamedArgsSingleStringList {
  input (myArg: string[])
  output string
}

impl<llm, TestFnNamedArgsSingleStringList> v1 {
  client GPT35
  prompt #"
    Return this same value back: {#input.myArg}
  "#
}