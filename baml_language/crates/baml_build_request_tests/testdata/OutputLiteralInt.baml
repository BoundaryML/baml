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

function FnOutputLiteralInt(input: string) -> 5 {
  client GPT35
  prompt #"
    Return an integer: {{ ctx.output_format()}}
  "#
}

test TestOutputLiteralInt {
  functions [FnOutputLiteralInt]
  args {
    input "example input"
  }
}
