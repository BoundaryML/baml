

function TestOllama(input: string) -> string {
  client Claude
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
