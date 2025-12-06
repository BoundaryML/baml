class Foo {
  name string
}

function Bar(foo: Foo) -> Foo {
  client "openai/gpt-4o"
  prompt #"
    {{ ctx.output_format(quote_class_fields=true) }}
  "#
}
