// test for float
function TestFnNamedArgsSingleFloat_V2(myFloat: float) -> string {
  client GPT35
  prompt #"
    Return this value back to me: {{myFloat}}
  "#
}
