enum NamedArgsSingleEnum {
  ONE
  TWO
}

function FnTestNamedArgsSingleEnum {
  input (myArg: NamedArgsSingleEnum)
  output string
}

function FnTestNamedArgsSingleEnum_V2 {
  client GPT35
  prompt #"
    Print these values back to me:
    {{myArg}}
  "#
}