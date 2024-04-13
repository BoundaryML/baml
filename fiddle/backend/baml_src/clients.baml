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

client<llm> GPT3 {
  provider baml-openai-chat
  options {
    model gpt-3.5-turbo
    api_key env.OPENAI_API_KEY
  }
}

client<llm> Claude {
  provider baml-anthropic-chat
  options {
    model_name claude-2.1
    api_key env.ANTHROPIC_API_KEY
  }
}

client<llm> ClaudeInstant {
  provider baml-anthropic-chat
  options {
    model_name claude-instant-1.2
    api_key env.ANTHROPIC_API_KEY
  }
}