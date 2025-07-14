class WeatherAPI2 {
  city string @description("the user's city")
  timeOfDay string @description("As an ISO8601 timestamp")
}

class SendEmail {
  emailTo string
  emailSubject string
  emailBody string
}

function ChooseOneTool(user_message: string) -> WeatherAPI2 | SendEmail {
  client GPT4o  
  prompt #" 
    Choose the right schema that contains all the information in this message:
    ---
    {{ user_message }}
    ---

    {# special macro to print the output schema. #}
    {{ ctx.output_format }}

    JSON:
  "# 
}

test TestOneFunc {
  functions [ChooseOneTool]
  args {
    user_message #"
      what's the weather in san francisco tomorrow april 23rd 2024 at 3pm?
    "#
  }
}

test TestOneFunc2 {
  functions [ChooseOneTool]
  args {
    user_message #"
      Send an email to John Doe with the subject 'Hello' and the body 'How are you doing?'
    "#
  }
}
