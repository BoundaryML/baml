class TestClassNested {
  prop1 string
  prop2 InnerClass
}

class InnerClass {
  prop1 string
  prop2 string
  inner InnerClass2
}

class InnerClass2 {
  prop2 int
  prop3 float
}

function FnOutputClassNested(input: string) -> TestClassNested {
  client GPT35
  prompt #"
    Return a made up json blob that matches this schema:
    {{ctx.output_format}}
    ---

    JSON:
  "#
}

function FnOutputClassNestedExprFunc(input: string) -> TestClassNested {
  return TestClassNested {
    prop1: "example string",
    prop2: InnerClass {
      prop1: "example string",
      prop2: "example string",
      inner: InnerClass2 {
        prop2: 540,
        prop3: 1.23
      }
    }
  };
}

test FnOutputClassNested {
  functions [FnOutputClassNested]
  args {
    input "example input"
  }
}
