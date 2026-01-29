class TestOutputClass {
  prop1 string @description("A long string with about 200 words")
  prop2 int
}

function FnOutputClass(input: string) -> TestOutputClass {
  client GPT35
  prompt #"
    Return a JSON blob with this schema: 
    {{ctx.output_format}}

    For the prop2, always return a 540

    JSON:
  "#
}

function FnOutputClassExprFunc(input: string) -> TestOutputClass {
  return TestOutputClass {
    prop1: "example string",
    prop2: 540
  };
}

test TestClass {
  functions [FnOutputClass]
  args {
    input "example input"
  }
}
