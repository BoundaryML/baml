class TestOutputClass {
  prop1 string
  prop2 int
}


function V2_FnOutputClass {
  (input: string  ) -> TestOutputClass
}

function FnOutputClass_V2 {
  client GPT35
  prompt #"
    Return a JSON blob with this schema: 
    {#print_type(output)}
    JSON:
  "#
}

