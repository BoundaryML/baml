class TestOutputClass {
  prop1 string
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

test TestClass {
  functions [FnOutputClass, FnOutputNestedClass]
  args {
    input "example input"
  }
}

 

class TestOutputClassNested {
  prop1 string
  prop2 int
  prop3 TestOutputClass
}

function FnOutputNestedClass(input: string) -> TestOutputClassNested {
  client GPT35
  prompt #"
    Return a JSON blob with this schema: 
    {{ctx.output_format}}

    JSON:
  "#
}