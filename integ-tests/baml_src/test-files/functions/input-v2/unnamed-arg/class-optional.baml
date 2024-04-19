class OptionalClassv2 {
  prop1 string
  prop2 string
}

function V2_FnClassOptional(input: OptionalClassv2?) -> string {
  client GPT35
  prompt #"
    {{ input }}
    Return three random words:
  "#
}

class ClassOptionalFieldsv2 {
  prop1 string?
  prop2 string?
}

function V2_FnClassOptional2(input: ClassOptionalFieldsv2) -> string {
  client GPT35
  prompt #"
    {{input}}
    Return three random words:
  "#
}

