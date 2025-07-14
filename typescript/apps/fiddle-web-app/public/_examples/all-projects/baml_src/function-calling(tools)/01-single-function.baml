class WeatherAPI {
  city string @description("the user's city")
  timeOfDay string @description("As an ISO8601 timestamp")
}

function UseTool(user_message: string) -> WeatherAPI {
  client GPT4o 
  prompt #"
    Extract the info from this message
    ---
    {{ user_message }}
    ---

    {# special macro to print the output schema. #}
    {{ ctx.output_format }}

    JSON:
  "#
}

test TestOne {
  functions [UseTool]
  args {
    user_message #"
      what's the weather in san francisco tomorrow april 23rd 2024 at 3pm?
    "#
  }
}
