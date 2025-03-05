function TestGemini(input: string) -> string {
  client Gemini
  prompt #"
    Write a nice short story about {{ input }}. Keep it to 15 words or less.
  "#
}

function TestGeminiSystem(input: string) -> string {
  client Gemini
  prompt #"
    {{ _.role('system') }}

    Write a nice short story about {{ input }}. Keep it to 15 words or less.
  "#
}

function TestGeminiSystemAsChat(input: string) -> string {
  client Gemini
  prompt #"
    {{ _.role('system') }} You are a helpful assistant

    {{_.role("user")}} Write a nice short story about {{ input }}. Keep it to 15 words or less.
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
