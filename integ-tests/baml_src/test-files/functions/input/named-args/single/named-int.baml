// test for int
function TestFnNamedArgsSingleInt(myInt: int) -> string {
  client GPT35
  prompt #"
    Return this value back to me: {{myInt}}
  "#
}

function TestFnNamedArgsSingleIntExprFunc(myInt: int) -> string {
  return "Hello, world!";
}

test TestFnNamedArgsSingleInt {
  functions [TestFnNamedArgsSingleInt]
  args {
    myInt 42
  }
}
