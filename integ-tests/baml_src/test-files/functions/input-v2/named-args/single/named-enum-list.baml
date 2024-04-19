enum NamedArgsSingleEnumList2 {
  ONE
  TWO
}

function V2_TestFnNamedArgsSingleEnumList(myArg: NamedArgsSingleEnumList2[]) -> string {
  client GPT35
  prompt #"
    Print these values back to me:
    {{myArg}}
  "#
}
