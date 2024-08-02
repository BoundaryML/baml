
retry_policy Exponential {
  max_retries 3
  strategy {
    type exponential_backoff
  }
}

retry_policy Constant {
  max_retries 3
  strategy {
    type constant_delay
    delay_ms 100
  }
}

client<llm> RetryClientConstant {
  provider openai
  retry_policy Constant
  options {
    model "gpt-3.5-turbo"
    api_key "blah"
  }
}

client<llm> RetryClientExponential {
  provider openai
  retry_policy Exponential
  options {
    model "gpt-3.5-turbo"
    api_key "blahh"
  }
}

function TestRetryConstant() -> string {
  client RetryClientConstant
  prompt #"
    Say a haiku
  "#
}

function TestRetryExponential() -> string {
  client RetryClientExponential
  prompt #"
    Say a haiku
  "#
}
