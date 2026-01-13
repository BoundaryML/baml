client<llm> GPT35 {
  provider openai
  options {
    model "gpt-4o-mini"
    api_key env.OPENAI_API_KEY
  }
}

class Blah {
  prop4 string?
}

class ClassOptionalOutput2 {
  prop1 string?
  prop2 string?
  prop3 Blah?
}

function FnOutputOptionalClassOptionalFields(input: string) -> ClassOptionalOutput2? {
  client GPT35
  prompt #"
    Return a json blob for the following input:
    {{input}}

    {{ctx.output_format}}

    JSON:
  "#
}

test TestOutputOptionalClassOptionalFields {
  functions [FnOutputOptionalClassOptionalFields]
  args {
    input "example input"
  }
}
