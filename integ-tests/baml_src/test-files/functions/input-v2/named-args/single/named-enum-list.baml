enum NamedArgsSingleEnumList {
  ONE
  TWO
}

function TestFnNamedArgsSingleEnumList {
  (myArg: NamedArgsSingleEnumList[]) -> string
}

function TestFnNamedArgsSingleEnumList_V2 {
  client GPT35
  prompt #"
    Print these values back to me:
    {{myArg}}
  "#
}