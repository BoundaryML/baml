// string[]
function V2_TestFnNamedArgsSingleStringArray {
  (myStringArray: string[]) -> string}

function V2_TestFnNamedArgsSingleStringArray_V2 {
  client GPT35
  prompt #"
    Return this value back to me: {{myStringArray}}
  "#
}
 
