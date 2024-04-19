enum NamedArgsSingleEnum {
  ONE
  TWO
}

function V2_FnTestNamedArgsSingleEnum(myArg: NamedArgsSingleEnum) -> string {
  client GPT35
  prompt #"
    Print these values back to me:
    {{myArg}}
  "#
}
