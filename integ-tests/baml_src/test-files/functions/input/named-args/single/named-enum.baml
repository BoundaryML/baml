enum NamedArgsSingleEnum {
  ONE
  TWO
}

function TestFnNamedArgsSingleEnum {
  input (myArg: NamedArgsSingleEnum)
  output string
}

impl<llm, TestFnNamedArgsSingleEnum> v1 {
  client GPT35
  prompt #"
    Print these values back to me:
    {#input.myArg}
  "#
}