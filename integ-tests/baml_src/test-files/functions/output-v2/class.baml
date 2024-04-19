class TestOutputClass {
  prop1 string
  prop2 int
}


function FnOutputClass {
  input string  output TestOutputClass
}

impl<llm, FnOutputClass> v1 {
  client GPT35
  prompt #"
    Return a JSON blob with this schema: 
    {#print_type(output)}
    JSON:
  "#
}

