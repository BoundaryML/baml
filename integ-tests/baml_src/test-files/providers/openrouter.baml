function TestOpenRouterMistralSmall3_1_24b(input: string) -> string {
  client OpenRouterMistralSmall3_1_24b
  prompt #"
    Write a nice short story about {{ input }}. Keep it to 15 words or less.
  "#
}
 
 
test TestName {
  functions [TestOpenRouterMistralSmall3_1_24b]
  args {
    input #"
      hello world
    "#
  }
}
  
 

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