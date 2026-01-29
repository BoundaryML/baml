function TestFnNamedArgsSingleFloat(myFloat: float) -> string {
  client GPT35
  prompt #"
    Return this value back to me: {{myFloat}}
  "#
}

function TestFnNamedArgsSingleFloatExprFunc(myFloat: float) -> string {
  return "Hello, world!";
}

test TestFnNamedArgsSingleFloat {
  functions [TestFnNamedArgsSingleFloat]
  args {
    myFloat 3.14
  }
}
