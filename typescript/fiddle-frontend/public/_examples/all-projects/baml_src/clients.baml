// These are LLM clients you can use in your functions. We currently support Anthropic, OpenAI / Azure, Gemini, and Ollama as providers but are expanding to many more.

// We also support any other provider that follows the OpenAI API specification, such as HuggingFace.

// For this playground, we have setup a few clients for you to use already with some free credits.

client<llm> GPT4 {
  // Use one of the following: https://docs.boundaryml.com/docs/snippets/clients/providers/openai
  provider openai
  // You can pass in any parameters from the OpenAI Python documentation into the options block.
  options {
    model gpt-4
    api_key env.OPENAI_API_KEY
  }
} 

client<llm> GPT4o {
  provider openai
  options {
    model gpt-4o
    api_key env.OPENAI_API_KEY
  }
} 

client<llm> Claude {
  provider anthropic
  options {
    model claude-3-haiku-20240307
    api_key env.ANTHROPIC_API_KEY
    max_tokens 1000

  }
}

client<llm> Gemini {
  provider google-ai
  options{
    model "gemini-1.5-pro-001"
    api_key env.GOOGLE_API_KEY
  }
}
