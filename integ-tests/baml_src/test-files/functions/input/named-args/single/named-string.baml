// test string
function TestFnNamedArgsSingleString(myString: string) -> string {
  client GPT35
  prompt #"
    Return this value back to me: {{myString}}
  "#
}

test TestFnNamedArgsSingleString {
  functions [TestFnNamedArgsSingleString]
  args {
    myString "example string"
  }
}
