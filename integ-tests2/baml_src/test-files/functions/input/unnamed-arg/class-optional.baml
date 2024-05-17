class OptionalClass {
  prop1 string
  prop2 string
}

function FnClassOptional {
  input OptionalClass?
  output string
}

impl<llm, FnClassOptional> v1 {
  client GPT35
  prompt #"
    {#input}

    Return three random words:
  "#
}

class ClassOptionalFields {
  prop1 string?
  prop2 string?
}

function FnClassOptional2 {
  input ClassOptionalFields
  output string
}

impl<llm, FnClassOptional2> v1 {
  client GPT35
  prompt #"
    {#input}

    Return three random words:
  "#
}

