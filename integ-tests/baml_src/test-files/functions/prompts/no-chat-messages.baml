

function PromptTestClaude(input: string) -> string {
  client Claude
  prompt #"
    Tell me a haiku about {{ input }}
  "#
}

function PromptTestOpenAI(input: string) -> string {
  client GPT35
  prompt #"
    Tell me a haiku about {{ input }}
  "#
}