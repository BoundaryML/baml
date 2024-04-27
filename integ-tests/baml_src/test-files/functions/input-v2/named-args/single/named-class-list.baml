class NamedArgsSingleClassList2 {
  key string
  key_two bool
  key_three int
  // TODO: doesn't work with keys with numbers
  // key2 bool
  // key3 int
}
function V2_TestFnNamedArgsSingleStringList(myArg: NamedArgsSingleClassList2[]) -> string {
  client GPT35
  prompt #"
    Return this same value back: {{myArg}}
  "#
}
