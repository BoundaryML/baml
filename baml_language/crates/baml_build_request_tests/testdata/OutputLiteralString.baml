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

function FnOutputLiteralString(input: string) -> "example output" {
  client GPT35
  prompt #"
    Return a string: {{ ctx.output_format()}}
  "#
}

test TestOutputLiteralString {
  functions [FnOutputLiteralString]
  args {
    input "example input"
  }
}
