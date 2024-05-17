class TestOutputClass2 {
  prop1 string
  prop2 int
}


function V2_FnOutputClass(input: string  ) -> TestOutputClass2 {
  client GPT35
  prompt #"
    Return a JSON blob with this schema: 
    {{ ctx.output_format }}
    JSON:
  "#
}

