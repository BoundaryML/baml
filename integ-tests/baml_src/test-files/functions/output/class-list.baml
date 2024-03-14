
function FnOutputClassList {
  input string
  output TestOutputClass[]
}

impl<llm, FnOutputClassList> v1 {
  client GPT35
  prompt #"
    Return a JSON array that follows this schema: 
    {#print_type(output)}

    JSON:
  "#
}
