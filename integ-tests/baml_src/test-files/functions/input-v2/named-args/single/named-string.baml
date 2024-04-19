// test string
function TestFnNamedArgsSingleString_V2(myString: string) -> string {
  client GPT35
  prompt #"
    Return this value back to me: {{myString}}
  "#
}
