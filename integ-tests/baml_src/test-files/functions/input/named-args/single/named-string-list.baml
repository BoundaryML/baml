// string[]
function FnNamedArgsSingleStringOptional {
  input (myString: string?)
  output string
}

impl<llm, FnNamedArgsSingleStringOptional> v1 {
  client GPT35
  prompt #"
    Return this value back to me: {#input.myString}
  "#
}
 