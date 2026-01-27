function FnOutputClassList(input: string) -> TestOutputClass[] {
  client GPT35
  prompt #"
    Return a JSON array that follows this schema: 
    {{ctx.output_format}}

    JSON:
  "#
}

function FnOutputClassListExprFunc(input: string) -> TestOutputClass[] {
  return [TestOutputClass {
    prop1: "example string",
    prop2: 540
  }];
}

test FnOutputClassList {
  functions [FnOutputClassList]
  args {
    input "example input"
  }
}
