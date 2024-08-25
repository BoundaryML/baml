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

function TestOpenAIShorthand(input: string) -> string {
  client "openai/gpt-4o-mini"
  prompt #"
    Write a nice short story about {{ input }}
  "#
}

function TestAnthropicShorthand(input: string) -> string {
  client "anthropic/claude-3-haiku-20240307"
  prompt #"
    Write a nice short story about {{ input }}
  "#
}

test TestProvider {
  functions [
    TestAnthropic, TestVertex, PromptTestOpenAI, TestAzure, TestOllama, TestGemini, TestAws,
    TestOpenAIShorthand,
    TestAnthropicShorthand
  ]
  args {
    input "Donkey kong and peanut butter"
  }
}


function TestCaching(input: string) -> string {
  client ClaudeWithCaching
  prompt #"
    {{ _.role('user', cache_control={"type": "ephemeral"}) }}
    Describe this in 5 words: {{ input }}
  "#
}
