function TestAnthropic(input: string) -> string {
  client Claude
  prompt #"
    Write a nice haiku about {{ input }}
  "#
}

function PromptTestOpenAI(input: string) -> string {
  client GPT35
  prompt #"
    Write a nice haiku about {{ input }}
  "#
}

function TestOpenAILegacyProvider(input: string) -> string {
  client GPT35LegacyProvider
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
    Write a nice short story about {{ input }}
  "#
}

function TestVertex(input: string) -> string {
  client Vertex
  prompt #"
    Write a nice short story about {{ input }}
  "#

}

function TestAws(input: string) -> string {
  client AwsBedrock
  prompt #"
    Write a nice short story about {{ input }}
  "#
}


test TestProvider {
  functions [TestAnthropic, TestVertex, TestOpenAI, TestAzure, TestOllama, TestGemini, TestAws]
  args {
    input "Donkey kong and peanut butter"
  }
}


