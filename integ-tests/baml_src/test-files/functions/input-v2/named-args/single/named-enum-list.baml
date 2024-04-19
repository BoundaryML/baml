enum NamedArgsSingleEnumList {
  ONE
  TWO
}

function TestFnNamedArgsSingleEnumList {
  input (myArg: NamedArgsSingleEnumList[])
  output string
}

function TestFnNamedArgsSingleEnumList_V2 {
  client GPT35
  prompt #"
    Print these values back to me:
    {{myArg}}
  "#
}