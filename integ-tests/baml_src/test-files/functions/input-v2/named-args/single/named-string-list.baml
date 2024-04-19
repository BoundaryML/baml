// string[]
function V2_FnNamedArgsSingleStringOptional_V2(myString: string?) -> string {
  client GPT35
  prompt #"
    Return this value back to me: {{myString}}
  "#
}
 
