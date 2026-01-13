retry_policy GPT4oRetry {
  max_retries 2
  strategy {
    type exponential_backoff
  }
}

client<llm> GPT35 {
  provider openai
  retry_policy GPT4oRetry
  options {
    model "gpt-4o-mini"
    api_key env.OPENAI_API_KEY
  }
}

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

test TestOutputClassNested {
  functions [FnOutputClassNested]
  args {
    input "example input"
  }
}
