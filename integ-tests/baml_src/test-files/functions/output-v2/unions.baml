class UnionTest_ReturnTypev2 {
  prop1 string | bool
  prop2 (float | bool)[]
  prop3 (float[] | bool[])
}

function V2_UnionTest_Function(input: string | bool  ) -> UnionTest_ReturnTypev2 {
  client GPT35
  prompt #"
    Return a JSON blob with this schema: 
    {{ ctx.output_format }}
    JSON:
  "#
}
