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

function ExtractResume(text: string) -> Resume {
  client $GENERATED_BAML_CLIENT$
  prompt #"
    Parse the following resume and return a structured representation of the data in the schema below.

    Resume:
    ---
    {{text}}
    ---

    {# output_format is a helper to print out Resume #}
    {{ ctx.output_format }}

    Output JSON:
  "#
}
