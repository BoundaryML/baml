// test for int
function V2_TestFnNamedArgsSingleInt(myInt: int) -> string {
  client GPT35
  prompt #"
    Return this value back to me: {{myInt}}
  "#
}
