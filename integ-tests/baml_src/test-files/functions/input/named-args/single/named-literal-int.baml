function TestNamedArgsLiteralInt(myInt: 1) -> string {
  client GPT35
  prompt #"
    Return this value back to me: {{myInt}}
  "#
}

function TestFnNamedArgsLiteralIntExprFunc(myInt: 1) -> string {
  return "Hello, world!";
}

test TestFnNamedArgsLiteralInt {
  functions [TestNamedArgsLiteralInt]
  args {
    myInt 1
  }
}