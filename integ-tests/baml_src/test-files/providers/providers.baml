function TestAnthropic(input: string) -> string {
  client Claude
  prompt #"
    Write a nice haiku about {{ input }}
  "#
}

function TestOpenAI(input: string) -> string {
  client GPT35
  prompt #"
    Write a nice haiku about {{ input }}
  "#
}

function TestAzure(input: string) -> string {
  client GPT35Azure
  prompt #"
    Write a nice haiku about {{ input }}
  "#
}

function TestOllama(input: string) -> string {
  client Ollama
  prompt #"
    Write a nice haiku about {{ input }}
  "#
}

function TestGemini(input: string) -> string {
  client Gemini
  prompt #"
    Write a nice haiku about {{ input }}
  "#
}


test TestProvider {
  functions [TestAnthropic, TestOpenAI, TestAzure, TestOllama, TestGemini]
  args {
    input "the moon"
  }
}


