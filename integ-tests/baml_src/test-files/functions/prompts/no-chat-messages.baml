

function PromptTestClaude(input: string) -> string {
  client Claude
  prompt #"
    Tell me a haiku about {{ input }}
  "#
}


function PromptTestStreaming(input: string) -> string {
  client GPT35
  prompt #"
    Tell me a short story about {{ input }}
  "#
}

test TestName {
  functions [PromptTestStreaming]
  args {
    input #"
      hello world
    "#
  }
}
