enum NamedArgsSingleEnumList {
  ONE
  TWO
}

function TestFnNamedArgsSingleEnumList {
  input (myArg: NamedArgsSingleEnumList[])
  output string
}

impl<llm, TestFnNamedArgsSingleEnumList> v1 {
  client GPT35
  prompt #"
    Print these values back to me:
    {#input.myArg}
  "#
}