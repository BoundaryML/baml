enum NamedArgsSingleEnumList {
  ONE
  TWO
}

function TestFnNamedArgsSingleEnumList(myArg: NamedArgsSingleEnumList[]) -> string {
  client GPT35
  prompt #"
    Print these values back to me:
    {{myArg}}
  "#
}

test TestFnNamedArgsSingleEnumList {
  functions [TestFnNamedArgsSingleEnumList]
  args {
    myArg [ONE, TWO]
  }
}