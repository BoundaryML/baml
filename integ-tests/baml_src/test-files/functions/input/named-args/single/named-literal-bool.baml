function TestNamedArgsLiteralBool(myBool: true) -> string {
  client GPT35
  prompt #"
    Return this value back to me: {{myBool}}
  "#
}

function TestFnNamedArgsLiteralBoolExprFunc(myBool: true) -> string {
  return "Hello, world!";
}

test TestFnNamedArgsLiteralBool {
  functions [TestNamedArgsLiteralBool]
  args {
    myBool true
  }
}