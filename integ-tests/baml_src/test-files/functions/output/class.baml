class TestOutputClass {
  prop1 string
  prop2 int
}

function FnOutputClass(input: string) -> TestOutputClass {
  client GPT35
  prompt #"
    Return a JSON blob with this schema: 
    {{ctx.output_format}}

    JSON:
  "#
}

test FnOutputClass {
  functions [FnOutputClass]
  args {
    input "example input"
  }
}
