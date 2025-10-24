
function ChooseNTools(user_message: string) -> (WeatherAPI2 | SendEmail)[] {
  client GPT4o  
  prompt #" 
    Choose the right schema(s) that contains all the information in this message:
    ---
    {{ user_message }}
    ---

    {# special macro to print the output schema. #}
    {{ ctx.output_format }}

    JSON:
  "# 
}

test TestNTools {
  functions [ChooseNTools]
  args {
    user_message #"
      what's the weather in san francisco tomorrow april 23rd 2024 at 3pm?

      Also send an email to Mark Gonzalez with the subject 'Hello' and the body 'How are you doing?'
    "#
  }
}
 