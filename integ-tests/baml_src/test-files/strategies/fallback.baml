
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
      Gemini
      Lottery_SimpleSyntax
      SecondaryFallback
      
    ]
  }
}

client<llm> SecondaryFallback {
  provider fallback
  options {
    strategy [
      FaultyClient
      RetryClientExponential
      GPT4
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

function TestRoundRobin() ->string{
  client Lottery_SimpleSyntax

  prompt #"
    Round robin haiku
  "#
}




test TestName {
  functions [TestFallbackClient, TestRoundRobin]
  args {

  }
}
