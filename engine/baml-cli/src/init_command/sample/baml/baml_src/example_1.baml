class Resume {
  name string
  education Education[]
  skills string[]
}

class Education {
  school string
  degree string
  year int
}

function ExtractResume {
  input string
  output Resume
}

impl<llm, ExtractResume> version1 {
  client $GENERATED_BAML_CLIENT$
  prompt #"
    Parse the following resume and return a structured representation of the data in the schema below.

    Resume:
    ---
    {#input}
    ---

    Output JSON format (only include these fields, and no others):
    {#print_type(output)}

    Output JSON:
  "#
}
