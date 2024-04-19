enum NamedArgsSingleEnum {
  ONE
  TWO
}

function FnTestNamedArgsSingleEnum {
  (myArg: NamedArgsSingleEnum) -> string
}

function FnTestNamedArgsSingleEnum_V2 {
  client GPT35
  prompt #"
    Print these values back to me:
    {{myArg}}
  "#
}