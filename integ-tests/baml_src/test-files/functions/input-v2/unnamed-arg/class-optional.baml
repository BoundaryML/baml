class OptionalClass {
  prop1 string
  prop2 string
}

function V2_FnClassOptional(input: OptionalClass?) -> string {
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

function V2_FnClassOptional2(input: ClassOptionalFields) -> string {
  client GPT35
  prompt #"
    {{input}}
    Return three random words:
  "#
}

