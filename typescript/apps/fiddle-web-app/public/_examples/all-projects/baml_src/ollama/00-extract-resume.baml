class OResume {
  name string
  education OEducation[] @description("Extract in the same order listed")
  skills string[] @description("Only include programming languages")
}

class OEducation {
  school string
  degree string
  year int
}

function ExtractResumeUsingOllama(resume_text: string) -> OResume {
  // see ollama-clients.baml
  client Llama3
  // client Mistral
  // client Gemma2
  // client Phi3

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

test OllamaTest1 {
  functions [ExtractResumeUsingOllama]
  args {
    resume_text #"
      John Doe

      Education
      - University of California, Berkeley
        - B.S. in Computer Science
        - 2020

      Skills
      - Python
      - Java
      - C++
    "#
  }
}