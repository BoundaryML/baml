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

function FnOutputLiteralBoolean(input: string) -> false {
  client GPT35
  prompt #"
    Return a false: {{ ctx.output_format()}}
  "#
}

test TestOutputLiteralBoolean {
  functions [FnOutputLiteralBoolean]
  args {
    input "example input"
  }
}
