
class ClassOptionalOutput {
  prop1 string
  prop2 string
}

function V2_FnClassOptionalOutput {
  input string  output ClassOptionalOutput?
}

impl<llm, FnClassOptionalOutput> v1 {
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

function V2_FnClassOptionalOutput2 {
  input string  output ClassOptionalOutput2?
}

impl<llm, FnClassOptionalOutput2> v1 {
  client GPT35
  prompt #"
    Return a json blob for the following input:    {#input}

    Answer in JSON using this schema:
    {#print_type(output)}
    JSON:
  "#
}