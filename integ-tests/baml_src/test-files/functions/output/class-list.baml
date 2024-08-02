function FnOutputClassList(input: string) -> TestOutputClass[] {
  client GPT35
  prompt #"
    Return a JSON array that follows this schema: 
    {{ctx.output_format}}

    JSON:
  "#
}

test FnOutputClassList {
  functions [FnOutputClassList]
  args {
    input "example input"
  }
}
