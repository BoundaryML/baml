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

test TestOutputClass {
  functions [FnOutputClass]
  args {
    input "example input"
  }
}
