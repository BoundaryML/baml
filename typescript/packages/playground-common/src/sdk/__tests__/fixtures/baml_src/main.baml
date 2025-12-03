// This is a BAML config file, which extends the Jinja2 templating language to write LLM functions.

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
  client GPT4o

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

function CheckAvailability(day: string) -> bool {
  client GPT4o

  prompt #"
    Is the office open on {{ day }}?
    The office is open Monday through Friday.

    Return true if open, false if closed.

    {{ ctx.output_format }}
  "#
}

function CountItems(text: string) -> int {
  client GPT4o

  prompt #"
    Count how many items are mentioned in the following text:

    {{ text }}

    {{ ctx.output_format }}
  "#
}

function ParseResume(resume_text: string) -> Resume {
  client GPT4o

  prompt #"
    Parse the resume below:

    {{ resume_text }}

    {{ ctx.output_format }}
  "#
}

test Test1 {
  functions [ExtractResume]
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

test CheckAvailabilityTest {
  functions [CheckAvailability]
  args {
    day "Monday"
  }
}

test ParseResumeTest {
  functions [ParseResume]
  args {
    resume_text #"
      Jane Smith

      Education
      - MIT
        - M.S. in AI
        - 2022

      Skills
      - Python
      - TypeScript
    "#
  }
}
