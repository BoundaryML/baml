function V2_TestFnNamedArgsSingleBool_V2(myBool: bool) -> string {
  client GPT35
  prompt #"
    Return this value back to me: {{myBool}}
  "#
}  
