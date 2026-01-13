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

class UnionTest_ReturnType {
  prop1 string | bool
  prop2 (float | bool)[]
  prop3 (bool[] | int[])
}

function FnOutputUnions(input: string | bool) -> UnionTest_ReturnType {
  client GPT35
  prompt #"
    Return a JSON blob with this schema:
    {{ctx.output_format}}

    JSON:
  "#
}

test TestOutputUnions {
  functions [FnOutputUnions]
  args {
    input "example input"
  }
}
