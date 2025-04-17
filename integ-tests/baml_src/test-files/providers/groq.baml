client<llm> Groq {
  provider openai-generic
  options {
    api_key env.GROQ_API_KEY
    model "llama-3.3-70b-versatile"
    base_url "https://api.groq.com/openai/v1"
  }
}

function TestGroq(input: string) -> string {
  client Groq
  prompt #"
    {{ _.role('user') }}
    {{ input }}
  "#
}