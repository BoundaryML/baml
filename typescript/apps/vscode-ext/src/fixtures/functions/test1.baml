client<llm> LLMClient {
  provider baml-openai-chat
  retry 4
  options {
    model gpt-3.5-turbo
  }
  
}