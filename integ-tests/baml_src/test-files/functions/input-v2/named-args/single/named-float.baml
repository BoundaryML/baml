// test for float
function V2_TestFnNamedArgsSingleFloat_V2(myFloat: float) -> string {
  client GPT35
  prompt #"
    Return this value back to me: {{myFloat}}
  "#
}
