class NamedArgsSingleClass {
  key string
  key_two bool
  key_three int
  // TODO: doesn't work with keys with numbers
  // key2 bool
  // key3 int
}

function V2_TestFnNamedArgsSingleClass {
  (myArg: NamedArgsSingleClass) -> string
}

function V2_TestFnNamedArgsSingleClass_V2 {
  client GPT35
  prompt #"
    Print these values back to me:
    {{myArg.key}}
    {{myArg.key_two}}
    {{myArg.key_three}}
  "#
}  