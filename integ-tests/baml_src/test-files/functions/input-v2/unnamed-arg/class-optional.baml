class OptionalClass {
  prop1 string
  prop2 string
}

function FnClassOptional {
  input OptionalClass?
  output string
}

function FnClassOptional_V2 {
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

function FnClassOptional2_V2(input: ClassOptionalFields) -> string {
  client GPT35
  prompt #"
    {{input}}

    Return three random words:
  "#
}

