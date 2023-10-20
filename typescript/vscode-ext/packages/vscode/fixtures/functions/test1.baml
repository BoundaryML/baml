client<llm> LLMClient {
  provider openai
  retry 4
  options {
    model gpt-3.5-turbo
  }
  
}