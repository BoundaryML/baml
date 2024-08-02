enum NamedArgsSingleEnum {
  ONE
  TWO
}

function FnTestNamedArgsSingleEnum(myArg: NamedArgsSingleEnum) -> string {
  client GPT35
  prompt #"
    Print these values back to me:
    {{myArg}}
  "#
}

test FnTestNamedArgsSingleEnum {
  functions [FnTestNamedArgsSingleEnum]
  args {
    myArg ONE
  }
}