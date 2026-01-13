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

function FnOutputLiteralUnions(input: string) -> 1 | true | "string output" {
  client GPT35
  prompt #"
    Return one of these values without any additional context:
    {{ctx.output_format}}
  "#
}

test TestOutputLiteralUnions {
  functions [FnOutputLiteralUnions]
  args {
    input "example input"
  }
}
