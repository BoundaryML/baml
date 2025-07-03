// OpenAI Responses Provider Validation Tests
// These tests validate that the openai-responses provider is properly configured

// Test 1: Basic provider recognition
// This should parse successfully once openai-responses is available
client<llm> ValidateOpenAIResponses {
  provider openai-responses
  options {
    api_key env.OPENAI_API_KEY
    model "gpt-4.1"
  }
}

// Test 2: Valid client_response_type values for openai-responses
client<llm> ValidateResponseTypeOpenAI {
  provider openai-responses
  options {
    api_key env.OPENAI_API_KEY
    model "gpt-4.1"
    client_response_type "openai"
  }
}

client<llm> ValidateResponseTypeOpenAIResponses {
  provider openai-responses
  options {
    api_key env.OPENAI_API_KEY
    model "gpt-4.1"
    client_response_type "openai-responses"
  }
}

client<llm> ValidateResponseTypeAnthropic {
  provider openai-responses
  options {
    api_key env.OPENAI_API_KEY
    model "gpt-4.1"
    client_response_type "anthropic"
  }
}

// Test 3: Provider should be in allowed list
// This will validate that "openai-responses" is included in ClientProvider::allowed_providers()

// Test 4: Default base URL should be correct
client<llm> ValidateDefaultBaseURL {
  provider openai-responses
  options {
    api_key env.OPENAI_API_KEY
    model "gpt-4.1"
    // Should default to https://api.openai.com/v1
  }
}

// Test 5: Custom base URL should work
client<llm> ValidateCustomBaseURL {
  provider openai-responses
  options {
    api_key env.OPENAI_API_KEY
    model "gpt-4.1"
    base_url "https://custom.openai.com/v1"
  }
}

// Simple test functions to validate the clients work
function ValidateBasicResponses(input: string) -> string {
  client ValidateOpenAIResponses
  prompt #"
    {{ _.role("user") }}
    Say "success" if you can read this: {{ input }}
  "#
}

function ValidateResponseTypes(input: string) -> string {
  client ValidateResponseTypeOpenAIResponses
  prompt #"
    {{ _.role("user") }}
    Respond with "response-type-works" for: {{ input }}
  "#
}

// Validation test suite
test ValidateOpenAIResponsesProvider {
  functions [
    ValidateBasicResponses,
    ValidateResponseTypes
  ]
  args {
    input "test"
  }
}