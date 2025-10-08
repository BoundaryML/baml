function PromptTestOpenAI(input: string) -> string {
  client GPT35
  prompt #"
    {{ _.role("user") }}
    Write a nice haiku, given the user input. Make sure to reference the input in the haiku.

    Input: {{ input }}
  "#
}

function TestOpenAILegacyProvider(input: string) -> string {
  client GPT35LegacyProvider
  prompt #"
    {{ _.role("user") }}
    Write a nice haiku, given the user input. Make sure to reference the input in the haiku.

    Input: {{ input }}
  "#
}

function TestOpenAIShorthand(input: string) -> string {
  client "openai/gpt-4o"
  prompt #"
    {{ _.role("user") }}
    Write a nice haiku, given the user input. Make sure to reference the input in the haiku.

    Input: {{ input }}
  "#
}




// Test standard GPT-4 (should add default max_tokens)
function TestOpenAI(input: string) -> string {
  client GPT4
  prompt #"
    {{ _.role("user") }}
    Write a nice haiku, given the user input. Make sure to reference the input in the haiku. Make it 50 paragraphs

    Input: {{ input }}
  "#
}

// Test O1 model without max_tokens (should not add default)
function TestOpenAIO1NoMaxTokens(input: string) -> string {
  client OpenAIO1
  prompt #"
    {{ _.role("user") }}
    Write a nice haiku, given the user input. Make sure to reference the input in the haiku.

    Input: {{ input }}
  "#
}

// Test O1 model with explicit max_tokens (should fail)
function TestOpenAIO1WithMaxTokens(input: string) -> string {
  client OpenAIO1WithMaxTokens
  prompt #"
    {{ _.role("user") }}
    Write a nice haiku, given the user input. Make sure to reference the input in the haiku.

    Input: {{ input }}
  "#
}

// Test O1 model with explicit max_completion_tokens
function TestOpenAIO1WithMaxCompletionTokens(input: string) -> string {
  client OpenAIO1WithMaxCompletionTokens
  prompt #"
    {{ _.role("user") }}
    Write a nice haiku, given the user input. Make sure to reference the input in the haiku.

    Input: {{ input }}
  "#
}

// Test GPT-4 with explicit max_tokens (should keep user value)
function TestOpenAIWithMaxTokens(input: string) -> string {
  client GPT4WithMaxTokens
  prompt #"
    {{ _.role("user") }}
    Write a nice haiku, given the user input. Make sure to reference the input in the haiku.

    Input: {{ input }}
  "#
}

// Test OpenAI with null max_tokens (should not add default)
function TestOpenAIWithNullMaxTokens(input: string) -> string {
  client OpenAIWithNullMaxTokens
  prompt #"
    {{ _.role("user") }}
    Write a nice haiku, given the user input. Make sure to reference the input in the haiku.

    Input: {{ input }}
  "#
}

client GPT4oMini {
  provider openai
  options {
    api_key env.OPENAI_API_KEY
    model "gpt-4o-mini"
  }
}

function TestOpenAIGPT4oMini(input: string) -> string {
  client GPT4oMini
  prompt #"
    {{ _.role("user") }}
    Write a nice haiku, given the user input. Make sure to reference the input in the haiku.
    {{ input }}
  "#
}
function TestOpenAIGPT4oMini2(input: string) -> string {
  client GPT4oMini
  prompt #"
    {{ _.role("user") }}
    Write a nice haiku, given the user input. Make sure to reference the input in the haiku.
  "#
}

function TestOpenAIGPT4oMini3(input: string) -> string {
  client GPT4oMini
  prompt #"
    {{ _.role("user") }}
    Write a nice haiku, given the user input. Make sure to reference the input in the haiku.
  "#
}

function OpenAIGPT4oMissingBaseUrlEnvVar(input: string) -> string {
  client GPT4oBaseUrlNotSet
  prompt #"
    {{ _.role("user") }}
    Write a nice haiku, given the user input. Make sure to reference the input in the haiku.
  "#
}

// Add test cases to verify the behavior
test TestOpenAIClients {
  functions [
    TestOpenAI,
    TestOpenAIO1NoMaxTokens,
    TestOpenAIO1WithMaxTokens,
    TestOpenAIWithMaxTokens,
    TestOpenAIO1WithMaxCompletionTokens,
    TestOpenAIWithNullMaxTokens
  ]
  args {
    input #"
    Cherry blossoms
    "#
  }
}

client<llm> OpenAIWithNullMaxTokens {
  provider openai
  options {
    api_key env.OPENAI_API_KEY
    model "gpt-4o"
    max_tokens null
  }
}