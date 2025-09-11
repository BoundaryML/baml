client<llm> GPT5 {
  provider openai-responses
  retry_policy Constant
  options {
    model gpt-5-chat-latest
    temperature 0.0
    allowed_roles ["system", "developer", "user", "assistant", "tool"]
    store false
  }
}

class Answer {
  answer string
}

// test string
function TestFnNamedArgsSingleString(myString: string) -> Answer {
  client GPT5
  prompt #"
    {{ _.role("system") }}
    Hi
    {{ _.role("user") }}
    Hi
    {{ _.role("assistant") }}
    Hello
    {{ _.role("user") }}
    QUESTION: Hi

    ----
    RELEVANT CONTEXT:
    hello

    {{ ctx.output_format }}

    RESPONSE:
  "#
}

test TestFnNamedArgsSingleString {
  functions [TestFnNamedArgsSingleString]
  args {
    myString "example string"
  }
}
