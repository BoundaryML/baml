function FooBar {
  input Foo|Foo2
  output Bar
}



enum Foo2 {
  VALUE @alias(foo)
  @meta(description, #"
  This is the description
  "#) 
  @meta(foo, #"car, bar, start"#)

  FIELD
  OTHERWISE

  // Values @serializer_template({
  //   value_template #""#
  //   deliminter \n
  // })
}