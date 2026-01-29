


function TestFnNamedArgsSingleStringList(myArg: string[]) -> string[] {
  client GPT35
  prompt #"
    Return this value back to me: {{myArg}}
  "#
}

function TestFnNamedArgsSingleStringListExprFunc(myArg: string[]) -> string[] {
  return ["hello", "world"];
}

test TestFnNamedArgsSingleStringList {
  functions [TestFnNamedArgsSingleStringList]
  args {
    myArg ["hello", "world"]
  }
}