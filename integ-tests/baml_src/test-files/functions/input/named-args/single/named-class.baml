class NamedArgsSingleClass {
  key string
  key_two bool
  key_three int
  // TODO: doesn't work with keys with numbers
  // key2 bool
  // key3 int
}

function TestFnNamedArgsSingleClass(myArg: NamedArgsSingleClass) -> string {
  client GPT35
  prompt #"
    Print these values back to me:
    {{myArg.key}}
    {{myArg.key_two}}
    {{myArg.key_three}}
  "#
}

test TestFnNamedArgsSingleClass {
  functions [TestFnNamedArgsSingleClass]
  args {
    myArg {
      key "example",
      key_two true,
      key_three 42
    }
  }
}
