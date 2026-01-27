function TestNamedArgsLiteralString(myString: "My String") -> string {
  client GPT35
  prompt #"
    Return this value back to me: {{myString}}
  "#
}

function TestFnNamedArgsLiteralStringExprFunc(myString: "My String") -> string {
  return "Hello, world!";
}

test TestFnNamedArgsLiteralString {
  functions [TestNamedArgsLiteralString]
  args {
    myString "My String"
  }
}