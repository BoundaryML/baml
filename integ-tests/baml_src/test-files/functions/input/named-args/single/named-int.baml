// test for int
function TestFnNamedArgsSingleInt {
  input (myInt: int)
  output string
}

impl<llm, TestFnNamedArgsSingleInt> v1 {
  client GPT35
  prompt #"
    Return this value back to me: {#input.myInt}
  "#
}