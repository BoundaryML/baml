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

function FnOutputClassList(input: string) -> TestOutputClass[] {
  client GPT35
  prompt #"
    Return a JSON array that follows this schema:
    {{ctx.output_format}}

    JSON:
  "#
}

test TestOutputClassList {
  functions [FnOutputClassList]
  args {
    input "example input"
  }
}
