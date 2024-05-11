// These are LLM clients you can use in your functions. We currently support Anthropic, OpenAI / Azure, and Ollama as providers but are expanding to many more.

// For this playground, we have setup a few clients for you to use already with some free credits.

client<llm> GPT4 {
  // Use one of the following: https://docs.boundaryml.com/v3/syntax/client/client#providers
  provider baml-openai-chat
  // You can pass in any parameters from the OpenAI Python documentation into the options block.
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
    max_tokens 1000

  }
}