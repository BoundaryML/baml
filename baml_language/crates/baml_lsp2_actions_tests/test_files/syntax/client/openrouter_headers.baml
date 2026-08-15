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
// E0010
//
//   × `client<llm>` config blocks are removed; declare a client value instead: `client OpenRouterMistralSmall3_1_24b = openai.GenericClient.new(model = "...", base_url = "...");` (compose reliability
//   │ with ai.Retry / ai.Fallback / ai.RoundRobin)
//     ╭─[client_openrouter_headers.baml:2:1]
//   2 │ ╭─▶ client<llm> OpenRouterMistralSmall3_1_24b {
//   3 │ │     provider "openai-generic"
//   4 │ │     options {
//   5 │ │       base_url "https://openrouter.ai/api/v1"
//   6 │ │       api_key env.OPENROUTER_API_KEY
//   7 │ │       model "mistralai/mistral-small-3.1-24b-instruct"
//   8 │ │       temperature 0.1
//   9 │ │       headers {
//  10 │ │         "HTTP-Referer" "https://me.com" // Optional
//  11 │ │         "X-Title" "me" // Optional
//  12 │ │       }
//  13 │ │     }
//  14 │ ├─▶ }
//     · ╰──── replace with `client Name = <expr>;`
//     ╰────
