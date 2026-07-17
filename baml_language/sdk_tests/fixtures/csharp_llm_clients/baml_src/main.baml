client<llm> Offline {
  provider openai
  options {
    model "gpt-4o-mini"
    api_key "offline-fixture-key"
  }
}

function Inspect(text: string) -> string {
  client Offline
  prompt #"
    Repeat the marker as JSON.
    Marker: {{ text }}
  "#
}
