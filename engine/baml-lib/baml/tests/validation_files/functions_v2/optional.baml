function FnNamedArgsSingleStringOptional(myString: string?) -> string {
  client GPT35
  prompt #"
    Return this value back to me: {{myString}}
  "#
}

test FnNamedArgsSingleStringOptional {
  functions [FnNamedArgsSingleStringOptional]
  args {
    myString "example string"
  }
}

// error: Client GPT35 not found. Did you mean one of these: `int`, `float`, `bool`?
//   -->  functions_v2/optional.baml:2
//    | 
//  1 | function FnNamedArgsSingleStringOptional(myString: string?) -> string {
//  2 |   client GPT35
//    | 
