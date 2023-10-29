function FooBar {
  input Foo2
  output Foo
}

enum Foo2 {
  VALUE
  @alias(foo)
  @meta(description, #"
  This is the description
  "#)
  @meta(foo, #"car, bar, start"#)

  FIELD
  @meta(foo, #""#)

  OTHERWISE
}
