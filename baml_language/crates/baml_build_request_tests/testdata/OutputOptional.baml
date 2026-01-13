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

class OptionalTest_Prop1 {
  omega_a string
  omega_b int
}

enum OptionalTest_CategoryType {
  Aleph
  Beta
  Gamma
}

class OptionalTest_ReturnType {
  omega_1 OptionalTest_Prop1?
  omega_2 string?
  omega_3 (OptionalTest_CategoryType?)[]
}

function FnOutputOptional(input: string) -> (OptionalTest_ReturnType?)[]
{
  client GPT35
  prompt #"
    Return a JSON blob with this schema:
    {{ctx.output_format}}

    JSON:
  "#
}

test TestOutputOptional {
  functions [FnOutputOptional]
  args {
    input "example input"
  }
}
