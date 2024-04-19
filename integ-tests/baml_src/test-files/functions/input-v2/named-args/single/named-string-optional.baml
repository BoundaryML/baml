// string[]
function TestFnNamedArgsSingleStringArray {
  input (myStringArray: string[])
  output string
}

function TestFnNamedArgsSingleStringArray_V2 {
  client GPT35
  prompt #"
    Return this value back to me: {{myStringArray}}
  "#
}
 