
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
      FaultyClient

      Lottery_SimpleSyntax
      // FaultyClient,

      // RetryClientConstant
      // RetryClientExponential
      // Gemini
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
      // RetryClientConstant
      // GPT4
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
client<llm> Lottery_SimpleSyntax {
  provider baml-round-robin
  retry_policy Constant
  options {
    strategy [
      GPT35
      Claude
      
    ]
  }
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
