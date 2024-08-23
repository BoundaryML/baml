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
  client Ollama
  prompt #"
    Return a made up json blob that matches this schema:
    {{ctx.output_format}}
    ---

    JSON:
  "#
}

test FnOutputClassNested {
  functions [FnOutputClassNested]
  args {
    input "example input"
  }
}
