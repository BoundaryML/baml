// Test function calling with OpenAI Responses API
// The 'enum' key should parse correctly in nested option maps
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

//----
//- diagnostics
// E0010
//
//   × `client<llm>` config blocks are removed; declare a client value instead: `client OpenAIResponsesFunctionCall = openai.ResponsesClient.new(model = "...");` (compose reliability with ai.Retry /
//   │ ai.Fallback / ai.RoundRobin)
//     ╭─[client_enum_key_in_options.baml:3:1]
//   3 │ ╭─▶ client<llm> OpenAIResponsesFunctionCall {
//   4 │ │     provider openai-responses
//   5 │ │     options {
//   6 │ │       api_key env.OPENAI_API_KEY
//   7 │ │       model "gpt-5-mini"
//   8 │ │       tools [
//   9 │ │         {
//  10 │ │           type "function"
//  11 │ │           name "get_current_weather"
//  12 │ │           description "Get the current weather in a given location"
//  13 │ │           parameters {
//  14 │ │             type "object"
//  15 │ │             properties {
//  16 │ │               location {
//  17 │ │                 type "string"
//  18 │ │                 description "The city and state, e.g. San Francisco, CA"
//  19 │ │               }
//  20 │ │               unit {
//  21 │ │                 type "string"
//  22 │ │                 enum ["celsius", "fahrenheit"]
//  23 │ │               }
//  24 │ │             }
//  25 │ │             required ["location", "unit"]
//  26 │ │           }
//  27 │ │         }
//  28 │ │       ]
//  29 │ │       tool_choice "auto"
//  30 │ │     }
//  31 │ ├─▶ }
//     · ╰──── replace with `client Name = <expr>;`
//     ╰────
