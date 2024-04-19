// test string
function V2_TestFnNamedArgsSingleString(myString: string) -> string {
  client GPT35
  prompt #"
    Return this value back to me: {{myString}}
  "#
}
