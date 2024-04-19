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

    Output JSON format (only include these fields, and no others):
    {# output_schema is inferred from the return type of the function #}
    {{ ctx.output_schema }}

    Output JSON:
  "#
}
