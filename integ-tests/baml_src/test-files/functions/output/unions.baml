class UnionTest_ReturnType {
  prop1 string | bool
  prop2 (float | bool)[]
  prop3 (float[] | bool[])
}

function UnionTest_Function(input: string | bool) -> UnionTest_ReturnType {
  client GPT35
  prompt #"
    Return a JSON blob with this schema: 
    {{ctx.output_format}}

    JSON:
  "#
}

test UnionTest_Function {
  functions [UnionTest_Function]
  args {
    input "example input"
  }
}
