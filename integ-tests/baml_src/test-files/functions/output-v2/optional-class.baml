class Blah2 {
  prop4 string?
}

class ClassOptionalOutput2v2 {
  prop1 string?
  prop2 string?
  prop3 Blah2?
}

function FnClassOptionalOutput2_V2(input: string) -> ClassOptionalOutput2v2? {
  client GPT35
  prompt #"
    Return a json blob for the following input:    {{input}}

    Answer in JSON using this schema:
    {{ ctx.output_format }}
    JSON:
  "#
}
