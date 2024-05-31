// test for int
function TestFnNamedArgsSingleInt(myInt: int) -> string {
  client GPT35
  prompt #"
    Return this value back to me: {{myInt}}
  "#
}

test TestFnNamedArgsSingleInt {
  functions [TestFnNamedArgsSingleInt]
  args {
    myInt 42
  }
}
