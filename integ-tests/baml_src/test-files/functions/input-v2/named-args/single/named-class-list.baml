function V2_TestFnNamedArgsSingleStringList(myArg: string[]) -> string {
  client GPT35
  prompt #"
    Return this same value back: {{myArg}}
  "#
}
