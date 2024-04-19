// string[]
function TestFnNamedArgsSingleStringArray {
  input (myStringArray: string[])
  output string
}

impl<llm, TestFnNamedArgsSingleStringArray> v1 {
  client GPT35
  prompt #"
    Return this value back to me: {{myStringArray}}
  "#
}
 