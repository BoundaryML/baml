// Test OpenRouter client with custom headers
client<llm> OpenRouterMistralSmall3_1_24b {
  provider "openai-generic"
  options {
    base_url "https://openrouter.ai/api/v1"
    api_key env.OPENROUTER_API_KEY
    model "mistralai/mistral-small-3.1-24b-instruct"
    temperature 0.1
    headers {
      "HTTP-Referer" "https://me.com" // Optional
      "X-Title" "me" // Optional
    }
  }
}

//----
//- diagnostics
// <no-diagnostics-expected>
