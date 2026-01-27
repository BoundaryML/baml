

 // string[]
function FnNamedArgsSingleStringOptional(myString: string?) -> string {
  client GPT35
  prompt #"
    Return this value back to me: {{myString}}
  "#
}

function FnNamedArgsSingleStringOptionalExprFunc(myString: string?) -> string {
  return "Hello, world!";
}

test FnNamedArgsSingleStringOptional {
  functions [FnNamedArgsSingleStringOptional]
  args {
    myString "example string"
  }
}

test FnNamedArgsSingleStringOptional2 {
  functions [FnNamedArgsSingleStringOptional]
  args {
    
  }
}
