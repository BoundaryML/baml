class UnionTest_ReturnType {
  prop1 string | bool
  prop2 (float | bool)[]
  prop3 (float[] | bool[])
}

function V2_UnionTest_Function {
}

function V2_UnionTest_Function(input: string | bool  ) -> UnionTest_ReturnType {
  client GPT35
  prompt #"
    Return a JSON blob with this schema: 
    {#print_type(output)}
    JSON:
  "#
}
