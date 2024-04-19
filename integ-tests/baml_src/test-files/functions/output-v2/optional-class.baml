
class ClassOptionalOutput {
  prop1 string
  prop2 string
}

function FnClassOptionalOutput_V2 {
  (input: string  ) -> ClassOptionalOutput? {
  client GPT35
  prompt #"
    Return a json blob for the following input:    {#input}

    Answer in JSON using this schema:
    {#print_type(output)}
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

function FnClassOptionalOutput2_V2(input: string  ) -> ClassOptionalOutput2? {
  client GPT35
  prompt #"
    Return a json blob for the following input:    {#input}

    Answer in JSON using this schema:
    {#print_type(output)}
    JSON:
  "#
}
