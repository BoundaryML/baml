// string[]
function TestFnNamedArgsSingleStringArray {
  (myStringArray: string[]) -> string}

function TestFnNamedArgsSingleStringArray_V2 {
  client GPT35
  prompt #"
    Return this value back to me: {{myStringArray}}
  "#
}
 
