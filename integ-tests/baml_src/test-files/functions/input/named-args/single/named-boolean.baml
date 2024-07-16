

function TestFnNamedArgsSingleBool(myBool: bool) -> string{
  client Vertex
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