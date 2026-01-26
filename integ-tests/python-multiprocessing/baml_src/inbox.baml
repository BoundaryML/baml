class Foo {
  reasoning string
}

function MakeFoo(
  subject: string,
  from_email: string,
  body: string,
) -> Foo {
  client CustomHaiku
  prompt #"
    Make a random value.

    {{ ctx.output_format }}
  "#
}
