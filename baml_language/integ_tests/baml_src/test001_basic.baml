class Test001Basic {
  name string
  skills string[]
}

function ExtractTest001Basic(text: string) -> Test001Basic {
  client GPT4oMini
  prompt #"
    Extract a resume from the following text.
    Return the name and a list of skills.

    Respond in JSON with this exact schema:
    {"name": "string", "skills": ["string", ...]}

    Text:
    {{ text }}

    JSON:
  "#
}
