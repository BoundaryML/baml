
function LlmEcho(input: string) -> string {
  // see clients.baml
  client "openai/gpt-4o-mini"

  // The prompt uses Jinja syntax. Change the models or this text and watch the prompt preview change!
  prompt #"
    Repeat the following text back to me: {{ input }}
  "#
}