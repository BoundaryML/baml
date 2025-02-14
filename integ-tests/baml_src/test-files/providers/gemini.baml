function TestGemini(input: string) -> string {
  client Gemini
  prompt #"
    Write a nice short story about {{ input }}
  "#
}

function TestGeminiSystem(input: string) -> string {
  client Gemini
  prompt #"
    {{ _.role('system') }}

    Write a nice short story about {{ input }}
  "#
}

function TestGeminiSystemAsChat(input: string) -> string {
  client Gemini
  prompt #"
    {{ _.role('system') }} You are a helpful assistant

    {{_.role("user")}} Write a nice short story about {{ input }}
  "#
}

function TestGeminiOpenAiGeneric() -> string {
  client GeminiOpenAiGeneric
  prompt #"{{_.role("system")}} You are a helpful assistant
  {{_.role("user")}} Write a poem about llamas
  "#
}

test TestName {
  functions [TestGeminiSystem]
  args {
    input #"
      hello world
    "#
  }
}
