class ClassOptionalOutput {
  prop1 string
  prop2 string
}

function FnClassOptionalOutput(input: string) -> ClassOptionalOutput? {
  client GPT35
  prompt #"
    Return a json blob for the following input:
    {{input}}

    {{ctx.output_format}}

    JSON:
  "#
}


class Blah {
  prop4 string?
}

class ClassOptionalOutput2 {
  prop1 string?
  prop2 string?
  prop3 Blah?
}

function FnClassOptionalOutput2(input: string) -> ClassOptionalOutput2? {
  client GPT35
  prompt #"
    Return a json blob for the following input:
    {{input}}

    {{ctx.output_format}}

    JSON:
  "#
}

test FnClassOptionalOutput2 {
  functions [FnClassOptionalOutput2, FnClassOptionalOutput]
  args {
    input "example input"
  }
}
