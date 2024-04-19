enum NamedArgsSingleEnum2 {
  ONE
  TWO
}

function V2_FnTestNamedArgsSingleEnum(myArg: NamedArgsSingleEnum2) -> string {
  client GPT35
  prompt #"
    Print these values back to me:
    {{myArg}}
  "#
}
