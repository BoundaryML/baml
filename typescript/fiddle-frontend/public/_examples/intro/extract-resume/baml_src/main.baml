// This is a BAML config file, which extends the Jinja2 templating language to write LLM functions.

// BAML adds many new features to Jinja:
// - type-support,
// - static analysis of prompts, 
// - robust deserialization of JSON outputs,
// - ...and more! 

class Resume {
  name string
  education Education[] @description("Extract in the same order listed")
  skills string[] @description("Only include programming languages")
}

class Education {
  school string
  degree string
  year int
}

function ExtractResume(resume_text: string) -> Resume {
  // see clients.baml
  client GPT4Turbo

  // The prompt uses Jinja syntax. Change the models or this text and watch the prompt preview change!
  prompt #"
    Parse the following resume and return a structured representation of the data in the schema below.

    Resume:
    ---
    {{ resume_text }}
    ---

    {# special macro to print the output instructions. #}
    {{ ctx.output_format }}

    JSON:
  "#
}
// Open main.py to see how to use this function