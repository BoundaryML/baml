// Test standard Azure GPT-3.5 (should add default max_tokens)
function TestAzure(input: string) -> string {
  client GPT35Azure
  prompt #"
    {{ _.role("user") }}
    Write a nice haiku, given the user input. Make sure to reference the input in the haiku.

    Input: {{ input }}
  "#
}

// Test O1 model without max_tokens (should not add default)
function TestAzureO1NoMaxTokens(input: string) -> string {
  client AzureO1
  prompt #"
   {{ _.role("user") }}
    Write a nice haiku, given the user input. Make sure to reference the input in the haiku.

    Input: {{ input }}
  "#
}

// Test O1 model with explicit max_tokens (should keep user value)
function TestAzureO1WithMaxTokens(input: string) -> string {
  client AzureO1WithMaxTokens
  prompt #"
    {{ _.role("user") }}
    Write a nice haiku, given the user input. Make sure to reference the input in the haiku.

    Input: {{ input }}
  "#
}

// Test O1 model with explicit max_completion_tokens (should keep user value)
function TestAzureO1WithMaxCompletionTokens(input: string) -> string {
  client AzureO1WithMaxCompletionTokens
  prompt #"
     {{ _.role("user") }}
    Write a nice haiku, given the user input. Make sure to reference the input in the haiku.

    Input: {{ input }}
  "#
}

// Test GPT-3.5 with explicit max_tokens (should keep user value)
function TestAzureWithMaxTokens(input: string) -> string {
  client GPT35AzureWithMaxTokens
  prompt #"
    {{ _.role("user") }}
    Write a nice haiku, given the user input. Make sure to reference the input in the haiku.

    Input: {{ input }}
  "#
}

// Test failure case with invalid resource name
function TestAzureFailure(input: string) -> string {
  client GPT35AzureFailed
  prompt #"
    {{ _.role("user") }}
    Write a nice haiku, given the user input. Make sure to reference the input in the haiku.

    Input: {{ input }}
  "#
}

client<llm> AzureWithNullMaxTokens {
  provider azure-openai
  options {
    resource_name env.AZURE_OPENAI_RESOURCE_NAME
    deployment_id env.AZURE_OPENAI_DEPLOYMENT_ID
    api_version "2024-02-01"
    max_tokens null
  }
}

// Test O3 model without max_tokens (should not add default)
function TestAzureO3NoMaxTokens(input: string) -> string {
  client AzureO3
  prompt #"
    {{ _.role("user") }}
    Write a nice haiku, given the user input. Make sure to reference the input in the haiku.

    Input: {{ input }}
  "#
}

// Test O3 model with explicit max_completion_tokens (should keep user value)
function TestAzureO3WithMaxCompletionTokens(input: string) -> string {
  client AzureO3WithMaxCompletionTokens
  prompt #"
    {{ _.role("user") }}
    Write a nice haiku, given the user input. Make sure to reference the input in the haiku.

    Input: {{ input }}
  "#
}

// Add test cases to verify the behavior
test TestAzureClients {
  functions [
    TestAzure,
    TestAzureO1NoMaxTokens,
    TestAzureO1WithMaxTokens,
    TestAzureWithMaxTokens,
    TestAzureO1WithMaxCompletionTokens,
    TestAzureO3NoMaxTokens,
    TestAzureO3WithMaxCompletionTokens
  ]
  args {
    input "Cherry blossoms"
  }
}

// Test failure case separately
test TestAzureFailureCase {
  functions [TestAzureFailure]
  args {
    input "Cherry blossoms"
  }
}