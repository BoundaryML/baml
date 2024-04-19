enum NamedArgsSingleEnum {
  ONE
  TWO
}

function FnTestNamedArgsSingleEnum {
  input (myArg: NamedArgsSingleEnum)
  output string
}

impl<llm, FnTestNamedArgsSingleEnum> v1 {
  client GPT35
  prompt #"
    Print these values back to me:
    {{myArg}}
  "#
}