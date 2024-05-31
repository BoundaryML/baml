


function TestFnNamedArgsSingleStringList(myArg: string[]) -> string{
  client GPT35
  prompt #"
    Return this value back to me: {{myArg}}
  "#
}

test TestFnNamedArgsSingleStringList {
  functions [TestFnNamedArgsSingleStringList]
  args {
    myArg ["hello", "world"]
  }
}