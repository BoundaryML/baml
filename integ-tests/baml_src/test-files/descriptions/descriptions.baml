
class Nested {
  prop3 string | null @description(#"
    write "three"
  "#)
  prop4 string | null @description(#"
    write "four"
  "#) @alias("blah")
  prop20 Nested2
}

class Nested2 {
  prop11 string | null @description(#"
    write "three"
  "#)
  prop12 string | null @description(#"
    write "four"
  "#) @alias("blah")
}

class Schema {
  prop1 string | null @description(#"
    write "one"
  "#)
  prop2 Nested | string @description(#"
    write "two"
  "#)
  prop5 (string | null)[] @description(#"
    write "hi"
  "#)
  prop6 string | Nested[] @alias("blah") @description(#"
    write the string "blah" regardless of the other types here
  "#)
  nested_attrs (string | null | Nested)[] @description(#"
    write the string "nested" regardless of other types
  "#)
  parens (string | null) @description(#"
    write "parens1"
  "#)
  other_group (string | (int | string)) @description(#"
    write "other"
  "#) @alias(other)
}


function SchemaDescriptions(input: string) -> Schema {
  client GPT4o
  prompt #"
    Return a schema with this format:

    {{ctx.output_format}}
  "#
}