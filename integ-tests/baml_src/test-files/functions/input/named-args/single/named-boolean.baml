

function TestFnNamedArgsSingleBool(myBool: bool) -> string{
  client GPT35
  prompt #"
    Return this value back to me: {{myBool}}
  "#
}

test TestFnNamedArgsSingleBool {
  functions [TestFnNamedArgsSingleBool]
  args {
    myBool true
  }
}