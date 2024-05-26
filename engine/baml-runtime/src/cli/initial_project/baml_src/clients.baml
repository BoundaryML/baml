client<llm> GPT4 {
  provider openai
  options {
    model "gpt-4"
    api_key env.OPENAI_API_KEY
  }
}

client<llm> Claude {
  provider anthropic
  options {
    model "claude-3-opus-20240229"
    api_key env.ANTHROPIC_API_KEY
  }
}


client<llm> FastAnthropic {
  provider anthropic
  options {
    model "claude-3-haiku-20240307"
    api_key env.ANTHROPIC_API_KEY
  }
}

client<llm> FastOpenAI {
  provider openai
  options {
    model "gpt-3.5-turbo"
    api_key env.OPENAI_API_KEY
  }
}


client<llm> Fast {
  provider round-robin
  options {
    // This will alternate between the two clients
    strategy [FastAnthropic, FastOpenAI]
  }
}

client<llm> Openai {
  provider fallback
  options {
    // This will try the clients in order until one succeeds
    strategy [GPT4, FastOpenAI]
  }
}