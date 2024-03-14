class NamedArgsSingleClass {
  key string
  key_two bool
  key_three int
  // TODO: doesn't work with keys with numbers
  // key2 bool
  // key3 int
}

function TestFnNamedArgsSingleClass {
  input (myArg: NamedArgsSingleClass)
  output string
}

impl<llm, TestFnNamedArgsSingleClass> v1 {
  client GPT35
  prompt #"
    Print these values back to me:
    {#input.myArg.key}
    {#input.myArg.key_two}
    {#input.myArg.key_three}
  "#
}