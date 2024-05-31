// string[]
function TestFnNamedArgsSingleStringArray(myStringArray: string[]) -> string {
  client GPT35
  prompt #"
    Return this value back to me: {{myStringArray}}
  "#
}

test TestFnNamedArgsSingleStringArray {
  functions [TestFnNamedArgsSingleStringArray]
  args {
    myStringArray ["example1", "example2", "example3"]
  }
}
