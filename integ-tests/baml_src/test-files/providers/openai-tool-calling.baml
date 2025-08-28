// OpenAI Tool Calling Integration Tests
// Tests the new baml_mode: "tool_calling" functionality

// Simple structured output for weather information
class WeatherInfo {
  temperature float @description("Temperature in degrees")
  units string @description("Either 'celsius' or 'fahrenheit'")
  condition string @description("Weather condition like 'sunny', 'cloudy', 'rainy'")
  humidity float? @description("Optional humidity percentage")
}

// Test basic tool calling with a simple class
function TestToolCallingSingle(city: string) -> WeatherInfo {
  client<llm> GPT4ToolCallingSingle {
    provider openai
    options {
      model "gpt-4o"
      api_key env.OPENAI_API_KEY
      baml_mode "tool_calling"
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
      tool_choice "auto"
    }
  }
  prompt #"
    What's the current weather in {{ city }}?
    
    Please provide the temperature, units, condition, and humidity if available.
  "#
}

// Test tool calling with required tool choice
function TestToolCallingRequired(city: string) -> WeatherInfo {
  client<llm> GPT4ToolCallingRequiredWeather {
    provider openai
    options {
      model "gpt-4o"
      api_key env.OPENAI_API_KEY
      baml_mode "tool_calling"
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
      tool_choice "required"
    }
  }
  prompt #"
    What's the current weather in {{ city }}?
  "#
}

// Multiple tool calls for array return type
function TestToolCallingMultiple(cities: string[]) -> WeatherInfo[] {
  client<llm> GPT4ToolCallingMultiple {
    provider openai
    options {
      model "gpt-4o"
      api_key env.OPENAI_API_KEY
      baml_mode "tool_calling"
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
      tool_choice "auto"
    }
  }
  prompt #"
    Get the weather for these cities: {{ cities | join(", ") }}
    
    Return weather information for each city.
  "#
}

// Test union return types with multiple tools
class SearchResult {
  query string @description("The search query")
  results string[] @description("Search results")
  count int @description("Number of results found")
}

class ImageGeneration {
  prompt string @description("The image generation prompt")
  image_url string @description("Generated image URL")
  size string @description("Image size")
}

function TestToolCallingUnion(instruction: string) -> SearchResult | ImageGeneration {
  client<llm> GPT4ToolCallingUnion {
    provider openai
    options {
      model "gpt-4o"
      api_key env.OPENAI_API_KEY
      baml_mode "tool_calling"
      tools [
        {
          type "function"
          function {
            name "SearchResult"
            description "Search for information"
            parameters {
              type "object"
              properties {
                query { type "string" }
                results { 
                  type "array"
                  items { type "string" }
                }
                count { type "number" }
              }
              required ["query", "results", "count"]
            }
          }
        },
        {
          type "function"
          function {
            name "ImageGeneration"
            description "Generate an image"
            parameters {
              type "object"
              properties {
                prompt { type "string" }
                image_url { type "string" }
                size { type "string" }
              }
              required ["prompt", "image_url", "size"]
            }
          }
        }
      ]
      tool_choice "auto"
    }
  }
  prompt #"
    {{ instruction }}
    
    Based on the instruction, either search for information or generate an image.
  "#
}

// Test with enum return type
enum Sentiment {
  POSITIVE
  NEGATIVE
  NEUTRAL
}

function TestToolCallingEnum(text: string) -> Sentiment {
  client<llm> GPT4ToolCallingEnum {
    provider openai
    options {
      model "gpt-4o"
      api_key env.OPENAI_API_KEY
      baml_mode "tool_calling"
      tools [
        {
          type "function"
          function {
            name "Sentiment"
            description "Analyze sentiment of text"
            parameters {
              type "string"
              enum ["POSITIVE", "NEGATIVE", "NEUTRAL"]
            }
          }
        }
      ]
      tool_choice "auto"
    }
  }
  prompt #"
    Analyze the sentiment of this text: {{ text }}
  "#
}

// Test nested class structure
class Address {
  street string
  city string
  country string
}

class Person {
  name string
  age int
  address Address
  hobbies string[]
}

function TestToolCallingNested(description: string) -> Person {
  client<llm> GPT4ToolCallingNested {
    provider openai
    options {
      model "gpt-4o"
      api_key env.OPENAI_API_KEY
      baml_mode "tool_calling"
      tools [
        {
          type "function"
          function {
            name "Person"
            description "Create a person profile"
            parameters {
              type "object"
              properties {
                name { type "string" }
                age { type "number" }
                address {
                  type "object"
                  properties {
                    street { type "string" }
                    city { type "string" }
                    country { type "string" }
                  }
                  required ["street", "city", "country"]
                }
                hobbies {
                  type "array"
                  items { type "string" }
                }
              }
              required ["name", "age", "address", "hobbies"]
            }
          }
        }
      ]
      tool_choice "auto"
    }
  }
  prompt #"
    Create a person profile based on this description: {{ description }}
  "#
}

// Test for comparison with traditional text_schema mode
function TestTextSchemaMode(city: string) -> WeatherInfo {
  client<llm> GPT4TextSchema {
    provider openai
    options {
      model "gpt-4o"
      api_key env.OPENAI_API_KEY
      baml_mode "text_schema" // Traditional mode
    }
  }
  prompt #"
    What's the current weather in {{ city }}?
    
    {{ ctx.output_format }}
  "#
}

// Test default mode (no baml_mode specified)
function TestDefaultMode(city: string) -> WeatherInfo {
  client<llm> GPT4Default {
    provider openai
    options {
      model "gpt-4o"
      api_key env.OPENAI_API_KEY
      // No baml_mode specified - should default to text_schema
    }
  }
  prompt #"
    What's the current weather in {{ city }}?
    
    {{ ctx.output_format }}
  "#
}