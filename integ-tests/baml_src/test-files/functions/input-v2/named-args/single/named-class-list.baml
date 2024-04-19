function V2_TestFnNamedArgsSingleStringList_V2(myArg: string[]) -> string {
  client GPT35
  prompt #"
    Return this same value back: {{myArg}}
  "#
}
