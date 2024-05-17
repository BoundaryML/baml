// test for float
function TestFnNamedArgsSingleFloat {
  input (myFloat: float)
  output string
}

impl<llm, TestFnNamedArgsSingleFloat> v1 {
  client GPT35
  prompt #"
    Return this value back to me: {#input.myFloat}
  "#
}