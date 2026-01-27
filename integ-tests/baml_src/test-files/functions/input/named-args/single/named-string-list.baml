// string[]
function TestFnNamedArgsSingleStringArray(myStringArray: string[]) -> string {
  client GPT35
  prompt #"
    Return this value back to me: {{myStringArray}}
  "#
}

function TestFnNamedArgsSingleStringArrayExprFunc(myStringArray: string[]) -> string {
  return "Hello, world!";
}

test TestFnNamedArgsSingleStringArray {
  functions [TestFnNamedArgsSingleStringArray]
  args {
    myStringArray ["example1", "example2", "example3"]
  }
}
