// OpenAI Responses API Provider Tests
// Tests the new openai-responses provider that uses the OpenAI Responses API

// Basic OpenAI Responses client
client<llm> OpenAIResponses {
  provider openai-responses
  options {
    api_key env.OPENAI_API_KEY
    model "gpt-5-mini"
  }
}

// OpenAI Responses client with explicit response type
client<llm> OpenAIResponsesExplicit {
  provider openai-responses
  options {
    api_key env.OPENAI_API_KEY
    model "gpt-5-mini"
    client_response_type "openai-responses"
  }
}

// OpenAI Responses client with custom base URL (for testing)
client<llm> OpenAIResponsesCustomURL {
  provider openai-responses
  options {
    api_key env.OPENAI_API_KEY
    model "gpt-5-mini"
    base_url "https://api.openai.com/v1"
  }
}

// Test basic functionality with responses API
function TestOpenAIResponses(input: string) -> string {
  client OpenAIResponses
  prompt #"
    {{ _.role("user") }}
    Write a short haiku about {{ input }}. Make it simple and beautiful.
  "#
}

// Test with explicit response type configuration
function TestOpenAIResponsesExplicit(input: string) -> string {
  client OpenAIResponsesExplicit
  prompt #"
    {{ _.role("user") }}
    Create a brief poem about {{ input }}. Keep it under 50 words.
  "#
}

// Test with custom base URL
function TestOpenAIResponsesCustomURL(input: string) -> string {
  client OpenAIResponsesCustomURL
  prompt #"
    {{ _.role("user") }}
    Tell me an interesting fact about {{ input }}.
  "#
}

// Test with multi-turn conversation
function TestOpenAIResponsesConversation(topic: string) -> string {
  client OpenAIResponses
  prompt #"
    {{ _.role("system") }}
    You are a helpful assistant that provides concise answers.
    
    {{ _.role("user") }}
    What is {{ topic }}?
    
    {{ _.role("assistant") }}
    {{ topic }} is a fascinating subject. Let me explain briefly.
    
    {{ _.role("user") }}
    Can you give me a simple example?
  "#
}

// Test with different model parameter
client<llm> OpenAIResponsesGPT4 {
  provider openai-responses
  options {
    api_key env.OPENAI_API_KEY
    model "gpt-4"
  }
}

function TestOpenAIResponsesDifferentModel(input: string) -> string {
  client OpenAIResponsesGPT4
  prompt #"
    {{ _.role("user") }}
    Explain {{ input }} in one sentence.
  "#
}

// Test error handling with invalid configuration
client<llm> OpenAIResponsesInvalidResponseType {
  provider openai-responses
  options {
    api_key env.OPENAI_API_KEY
    model "gpt-4.1"
    // This should work since openai response type is valid for responses provider
    client_response_type "openai"
  }
}

function TestOpenAIResponsesWithOpenAIResponseType(input: string) -> string {
  client OpenAIResponsesInvalidResponseType
  prompt #"
    {{ _.role("user") }}
    Write about {{ input }}.
  "#
}

// Comprehensive test suite for OpenAI Responses
test TestOpenAIResponsesProviders {
  functions [
    TestOpenAIResponses,
    TestOpenAIResponsesExplicit,
    TestOpenAIResponsesCustomURL,
    TestOpenAIResponsesConversation,
    TestOpenAIResponsesDifferentModel,
    TestOpenAIResponsesWithOpenAIResponseType
  ]
  args {
    input "mountains"
    topic "machine learning"
  }
}

// Test shorthand syntax (this should work but use standard openai, not responses)
function TestOpenAIResponsesShorthand(input: string) -> string {
  client "openai/gpt-5-mini"
  prompt #"
    {{ _.role("user") }}
    What do you think about {{ input }}?
  "#
}

// Test to ensure the provider correctly routes to /v1/responses endpoint
// This is validated by the implementation, not by the test execution
function TestOpenAIResponsesEndpoint(input: string) -> string {
  client OpenAIResponses
  prompt #"
    {{ _.role("user") }}
    This request should go to /v1/responses endpoint, not /v1/chat/completions.
    Respond with a short message about {{ input }}.
  "#
}

// Test that demonstrates automatic response type selection
client<llm> OpenAIResponsesAutoType {
  provider openai-responses
  options {
    api_key env.OPENAI_API_KEY
    model "gpt-5-mini"
    // No explicit client_response_type - should automatically use openai-responses
  }
}

function TestOpenAIResponsesAutoType(input: string) -> string {
  client OpenAIResponsesAutoType
  prompt #"
    {{ _.role("user") }}
    This client should automatically use openai-responses response type.
    Write a short description of {{ input }}.
  "#
}

// Additional test for validation
test TestOpenAIResponsesValidation {
  functions [
    TestOpenAIResponsesShorthand,
    TestOpenAIResponsesEndpoint,
    TestOpenAIResponsesAutoType,
    TestOpenAIResponsesExplicit,
    TestOpenAIProviderWithResponsesType
  ]
  args {
    input "artificial intelligence"
  }
}

// Test image input/output with OpenAI Responses API
client<llm> OpenAIResponsesImage {
  provider openai-responses
  options {
    api_key env.OPENAI_API_KEY
    model "gpt-5"
  }
}

function TestOpenAIResponsesImageInput(image: image | string | pdf | audio) -> string {
  client OpenAIResponsesImage
  prompt #"
    {{ _.role("user") }}
    what is in this content?
    {{ image }}
  "#
}

// Test for image analysis
test TestOpenAIResponsesImageAnalysis {
  functions [
    TestOpenAIResponsesImageInput
  ]
  args {
    image "https://upload.wikimedia.org/wikipedia/commons/thumb/d/dd/Gfp-wisconsin-madison-the-nature-boardwalk.jpg/2560px-Gfp-wisconsin-madison-the-nature-boardwalk.jpg"
  }
}

// Test web search with OpenAI Responses API
client<llm> OpenAIResponsesWebSearch {
  provider openai-responses
  options {
    api_key env.OPENAI_API_KEY
    model "gpt-5-mini"
    tools [
      {
        type "web_search_preview"
      }
    ]
  }
}

function TestOpenAIResponsesWebSearch(query: string) -> string {
  client OpenAIResponsesWebSearch
  prompt #"
    {{ _.role("user") }}
    {{ query }}
  "#
}

// Test for web search functionality
test TestOpenAIResponsesWebSearchTest {
  functions [
    TestOpenAIResponsesWebSearch
  ]
  args {
    query "What was a positive news story from today?"
  }
}


// Test function calling with OpenAI Responses API
client<llm> OpenAIResponsesFunctionCall {
  provider openai-responses
  options {
    api_key env.OPENAI_API_KEY
    model "gpt-5-mini"
    tools [
      {
        type "function"
        name "get_current_weather"
        description "Get the current weather in a given location"
        parameters {
          type "object"
          properties {
            location {
              type "string"
              description "The city and state, e.g. San Francisco, CA"
            }
            unit {
              type "string"
              enum ["celsius", "fahrenheit"]
            }
          }
          required ["location", "unit"]
        }
      }
    ]
    tool_choice "auto"
  }
}

function TestOpenAIResponsesFunctionCall(query: string) -> string {
  client OpenAIResponsesFunctionCall
  prompt #"
    {{ _.role("user") }}
    {{ query }}
  "#
}

// Test for function calling
test TestOpenAIResponsesFunctionCallTest {
  functions [
    TestOpenAIResponsesFunctionCall
  ]
  args {
    query "What is the weather like in Boston today?"
  }
}

// Test using standard openai provider with openai-responses client_response_type
client<llm> OpenAIWithResponsesType {
  provider openai
  options {
    api_key env.OPENAI_API_KEY
    model "gpt-5-mini"
    client_response_type "openai-responses"
  }
}

function TestOpenAIProviderWithResponsesType(input: string) -> string {
  client OpenAIWithResponsesType
  prompt #"
    {{ _.role("user") }}
    This uses the openai provider but with openai-responses client_response_type.
    Write a short summary about {{ input }}.
  "#
}

// Test reasoning with OpenAI Responses API
client<llm> OpenAIResponsesReasoning {
  provider openai-responses
  options {
    api_key env.OPENAI_API_KEY
    model "gpt-5"
    reasoning{
      effort "high"
    }
  }
}

function TestOpenAIResponsesReasoning(problem: string) -> string {
  client OpenAIResponsesReasoning
  prompt #"
    {{ _.role("user") }}
    {{ problem }}
  "#
}

// Test for reasoning capability
test TestOpenAIResponsesReasoningTest {
  functions [
    TestOpenAIResponsesReasoning
  ]
  args {
    problem "Solve this step by step: If a train travels at 60 mph for 2.5 hours, then at 80 mph for 1.5 hours, what is the total distance traveled?"
  }
}

client<llm> Gpt5 {
  provider openai-responses
  options {
    api_key env.OPENAI_API_KEY
    model "gpt-5"
  }
}


function TestOpenAIResponsesAllRoles(problem: string) -> string {
  client Gpt5
  prompt #"
    {{ _.role("system") }}
    Hi
    {{ _.role("developer") }}
    Hi
    {{ _.role("assistant") }}
    Hi
    {{ _.role("user") }}
    {{ problem }}
  "#
}


function TestOpenaiResponsesPdfs(pdf: pdf) -> string {
  client Gpt5
  prompt #"
    {{ _.role("user") }}
    Summarize in one sentence the contents of this:
    {{ pdf }}
  "#
} 

test TestOpenaiResponsesPdfsTest {
  functions [
    TestOpenaiResponsesPdfs
  ]
  args {
    pdf { url "https://www.berkshirehathaway.com/letters/2024ltr.pdf" }
  }
}

test TestOpenaiResponsesPdfsTestFile {
  functions [
    TestOpenaiResponsesPdfs
  ]
  args {
    pdf { file "../../dummy.pdf" }
  }
}


test TestOpenAIResponsesAllRolesTest {
  functions [
    TestOpenAIResponsesAllRoles
  ]
  args {
    problem "What is the weather like in Boston today?"
  }
}