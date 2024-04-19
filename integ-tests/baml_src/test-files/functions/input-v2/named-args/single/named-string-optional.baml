// string[]
function V2_TestFnNamedArgsSingleStringArray {
  (myStringArray: string[]) -> string}

function V2_TestFnNamedArgsSingleStringArray {
  client GPT35
  prompt #"
    Return this value back to me: {{myStringArray}}
  "#
}
 
