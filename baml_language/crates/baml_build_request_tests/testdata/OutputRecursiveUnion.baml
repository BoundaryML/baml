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

type RecursiveUnion = string | map<string, RecursiveUnion>

function FnOutputRecursiveUnion(input: RecursiveUnion) -> RecursiveUnion {
  client GPT35
  prompt #"
    Return back the same value you were given: {{ input }}

    {{ ctx.output_format() }}
  "#
}

test TestOutputRecursiveUnion {
  functions [FnOutputRecursiveUnion]
  args {
    input {
      "key" "value"
      "key2" {
        "key" "value2"
        "key2" "value3"
      }
    }
  }
}
