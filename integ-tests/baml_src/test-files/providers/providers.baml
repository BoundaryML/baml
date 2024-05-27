

function TestOllama(input: string) -> string {
  client Ollama
  prompt #"
    Write a nice haiku about {{ input }}
  "#
}

test TestProvider {
  functions [TestOllama]
  args {
    input "the moon"
  }
}