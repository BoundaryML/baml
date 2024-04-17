client<llm> GPT4 {
  provider baml-openai-chat
  options {
    model gpt-4
    api_key env.OPENAI_API_KEY
  }
} 

client<llm> GPT4Turbo {
  provider baml-openai-chat
  options {
    model gpt-4-1106-preview
    api_key env.OPENAI_API_KEY
  }
} 

client<llm> GPT35 {
  provider baml-openai-chat
  options {
    model gpt-3.5-turbo
    api_key env.OPENAI_API_KEY
  }
}  

client<llm> Claude {
  provider baml-anthropic-chat
  options {
    model claude-3-haiku-20240307
    api_key env.ANTHROPIC_API_KEY
  }
}

client<llm> Ollama {
  provider baml-anthropic-chat
  options {
    model claude-3-haiku-20240307
    api_key env.ANTHROPIC_API_KEY
  }
}

client<llm> Resilient_SimpleSyntax {
  provider baml-fallback
  options {
    strategy [
      GPT4Turbo
      GPT35
      Claude
    ]
  }
} 

client<llm> Resilient_ComplexSyntax {
  provider baml-fallback
  options {
    strategy [
      {
        client GPT4Turbo
      }
      {
        client GPT35
      }
      {
        client Claude
      }
    ]
  }
}

client<llm> Lottery_SimpleSyntax {
  provider baml-round-robin
  options {
    start 0
    strategy [
      GPT35
      Claude
    ]
  }
}

client<llm> Lottery_ComplexSyntax {
  provider baml-round-robin
  options {
    start 0
    strategy [
      {
        client GPT35
      }
      {
        client Claude
      }
    ]
  }
}