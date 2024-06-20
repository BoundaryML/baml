// This is a BAML config file, which extends the Jinja2 templating language to write LLM functions.

// BAML adds many new features to Jinja:
// - type-support,
// - static analysis of prompts, 
// - robust deserialization of JSON outputs,
// - ...and more! 


class Character {
  color string
  clothing string
}

function ExtractResume5(resume_text: image) -> Character {
  // see clients.baml
  client Gemini

  // The prompt uses Jinja syntax. Change the models or this text and watch the prompt preview change!
  prompt #"
   Describe the character

    Resume:
    ---
    {{ resume_text }}
    ---

    {# special macro to print the output instructions. #}
    {{ ctx.output_format }}

    JSON:
  "#
}

test Test1 {
  functions [ExtractResume5]
  args {
    resume_text {
          url "https://upload.wikimedia.org/wikipedia/en/4/4d/Shrek_%28character%29.png"

    }
  }
}