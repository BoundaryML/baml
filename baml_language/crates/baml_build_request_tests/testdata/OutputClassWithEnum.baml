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

enum EnumInClass {
  ONE
  TWO
}

class TestClassWithEnum {
  prop1 string
  prop2 EnumInClass
}

function FnOutputClassWithEnum(input: string) -> TestClassWithEnum {
  client GPT35
  prompt #"
    Return a made up json blob that matches this schema:
    {{ctx.output_format}}
    ---

    JSON:
  "#
}

test TestOutputClassWithEnum {
  functions [FnOutputClassWithEnum]
  args {
    input "example input"
  }
}
