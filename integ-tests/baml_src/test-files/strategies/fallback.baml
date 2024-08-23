
client<llm> FaultyClient {
  provider openai
  options {
    model unknown-model
    api_key env.OPENAI_API_KEY
  }
}


client<llm> FallbackClient {
  provider fallback
  options {
    // first 2 clients are expected to fail.
    strategy [
      FaultyClient,
      RetryClientConstant,
      GPT35
      Gemini

    ]
  }
}

function TestFallbackClient() -> string {
  client FallbackClient
  // TODO make it return the client name instead
  prompt #"
    Say a haiku about mexico.
  "#
}