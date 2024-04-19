// test for int
function TestFnNamedArgsSingleInt_V2(myInt: int) -> string {
  client GPT35
  prompt #"
    Return this value back to me: {{myInt}}
  "#
}
