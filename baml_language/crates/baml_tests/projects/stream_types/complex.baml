enum Color {
  Red
  Green
  Blue
}

class Inner {
  value int
}

type StringAlias = string

class Complex {
  color Color
  literal 5
  str_literal "hello"
  union_field int | string[]
  alias_field StringAlias
  nested_list Inner[][]
  optional_class Inner?
  union_with_null int | null | string
}

type AliasToClass = Inner
