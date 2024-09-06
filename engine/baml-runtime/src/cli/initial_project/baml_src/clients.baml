// Learn more about clients at https://docs.boundaryml.com/docs/snippets/clients/overview

client<llm> GPT4o {
  provider openai
  options {
    model "gpt-4o"
    api_key env.OPENAI_API_KEY
  }
}

client<llm> GPT4oMini {
  provider openai
  options {
    model "gpt-4o-mini"
    api_key env.OPENAI_API_KEY
  }
}

client<llm> Sonnet {
  provider anthropic
  options {
    model "claude-3-5-sonnet-20240620"
    api_key env.ANTHROPIC_API_KEY
  }
}


client<llm> Haiku {
  provider anthropic
  options {
    model "claude-3-haiku-20240307"
    api_key env.ANTHROPIC_API_KEY
  }
}

client<llm> Fast {
  provider round-robin
  options {
    // This will alternate between the two clients
    strategy [GPT4oMini, Haiku]
  }
}

client<llm> Openai {
  provider fallback
  options {
    // This will try the clients in order until one succeeds
    strategy [GPT4o, GPT4oMini]
  }
}