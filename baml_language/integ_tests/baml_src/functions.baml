class Resume {
  name string
  skills string[]
}

function ExtractResume(text: string) -> Resume {
  client GPT4oMini
  prompt #"
    Extract a resume from the following text.
    Return the name and a list of skills.

    {{ ctx.output_format }}

    Text:
    {{ text }}
  "#
}
