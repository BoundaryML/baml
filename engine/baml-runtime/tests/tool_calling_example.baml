// Example BAML file demonstrating tool calling

class WeatherInfo {
  temperature float
  units string @description("Either 'celsius' or 'fahrenheit'")
  condition string @description("Current weather condition like 'sunny', 'cloudy', 'rainy'")
  humidity float? @description("Humidity percentage, if available")
}

client<llm> GPT4ToolCalling {
  provider openai
  options {
    model "gpt-4o"
    api_key env.OPENAI_API_KEY
    baml_mode "tool_calling"
    tool_choice "auto"
    // Tools need to be manually specified for now
    tools [
      {
        type "function"
        function {
          name "WeatherInfo"
          description "Get weather information for a location"
          parameters {
            type "object"
            properties {
              temperature { type "number" }
              units { 
                type "string"
                enum ["celsius", "fahrenheit"]
              }
              condition { type "string" }
              humidity { type "number" }
            }
            required ["temperature", "units", "condition"]
          }
        }
      }
    ]
  }
}

function GetWeather(location: string) -> WeatherInfo {
  client GPT4ToolCalling
  prompt #"
    What's the current weather in {{ location }}?
    
    Please provide the weather information.
  "#
}

// Example with multiple tools (union return type)
class SearchResult {
  query string
  results string[]
  count int
}

class ImageGeneration {
  prompt string
  image_url string
  size string
}

function PerformAction(instruction: string) -> SearchResult | ImageGeneration {
  client GPT4ToolCalling
  prompt #"
    {{ instruction }}
    
    Based on the instruction, either search for information or generate an image.
  "#
}

// Test function without tool calling (for comparison)
function GetWeatherText(location: string) -> WeatherInfo {
  client<llm> GPT4Text {
    provider openai
    options {
      model "gpt-4o"
      api_key env.OPENAI_API_KEY
      baml_mode "text_schema" // or just omit baml_mode
    }
  }
  prompt #"
    What's the current weather in {{ location }}?
    
    {{ ctx.output_format }}
  "#
}