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
// Error: unreachable arm
//      ╭─[ llm_types.baml:159:17 ]
//      │
//  159 │             _ => { continue; }
//      │                 ───────┬──────  
//      │                        ╰──────── unreachable arm
//      │ 
//      │ Note: Error code: E0001
// ─────╯
// Error: unreachable arm
//      ╭─[ llm_types.baml:189:17 ]
//      │
//  189 │ ╭─▶             _ => {
//      ┆ ┆   
//  194 │ ├─▶             }
//      │ │                   
//      │ ╰─────────────────── unreachable arm
//      │     
//      │     Note: Error code: E0001
// ─────╯
// Error: unreachable arm
//      ╭─[ llm_types.baml:208:17 ]
//      │
//  208 │             _ => { continue; }
//      │                 ───────┬──────  
//      │                        ╰──────── unreachable arm
//      │ 
//      │ Note: Error code: E0001
// ─────╯
