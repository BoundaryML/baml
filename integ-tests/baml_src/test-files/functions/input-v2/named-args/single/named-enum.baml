enum NamedArgsSingleEnum {
  ONE
  TWO
}

function V2_FnTestNamedArgsSingleEnum {
  (myArg: NamedArgsSingleEnum) -> string
}

function V2_FnTestNamedArgsSingleEnum_V2 {
  client GPT35
  prompt #"
    Print these values back to me:
    {{myArg}}
  "#
}