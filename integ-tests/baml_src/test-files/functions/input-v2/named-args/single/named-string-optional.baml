// string[]
function V2_FnNamedArgsSingleStringOptional(myString: string?) -> string {
  client GPT35
  prompt #"
    Return this value back to me: {{myString}}
  "#
}
 
