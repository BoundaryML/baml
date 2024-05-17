// test string
function TestFnNamedArgsSingleString {
  input (myString: string)
  output string
}

impl<llm, TestFnNamedArgsSingleString> v1 {
  client GPT35
  prompt #"
    Return this value back to me: {#input.myString}
  "#
}