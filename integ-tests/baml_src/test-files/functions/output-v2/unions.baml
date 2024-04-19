class UnionTest_ReturnType {
  prop1 string | bool
  prop2 (float | bool)[]
  prop3 (float[] | bool[])
}

function V2_UnionTest_Function {
  input string | bool  output UnionTest_ReturnType
}

impl<llm, UnionTest_Function> v1 {
  client GPT35
  prompt #"
    Return a JSON blob with this schema: 
    {#print_type(output)}
    JSON:
  "#
}