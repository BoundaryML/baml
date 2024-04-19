enum NamedArgsSingleEnumList {
  ONE
  TWO
}

function V2_TestFnNamedArgsSingleEnumList {
  (myArg: NamedArgsSingleEnumList[]) -> string
}

function V2_TestFnNamedArgsSingleEnumList {
  client GPT35
  prompt #"
    Print these values back to me:
    {{myArg}}
  "#
}
