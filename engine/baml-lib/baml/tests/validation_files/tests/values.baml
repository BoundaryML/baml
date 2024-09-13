class Input {
  str_field string
  int_field int
  bool_field bool

  str_array_field string[]
  int_array_field int[]
  bool_array_field bool[]
}

function Foo(input: Input) -> string {
  client "openai/gpt-4o-mini"
  prompt #"Tell me a haiku."#
}

test Foo {
  functions [Foo]
  input {
    str_field "hello"
    int_field 123
    bool_field true

    str_array_field ["hello", "world"]
    int_array_field [1, 2, 3]
    bool_array_field [true, false]
  }
}