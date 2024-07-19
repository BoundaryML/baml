// here is a comment

class OtherClass {
  inner string
}

class MyClass {
  @@alias("this is an alias")

  field int
  field null
  field string
  field bool
  field float
  field OtherClass

  field OtherClass?

  field string[]

  field (int | string | OtherClass)
  field (int | string | OtherClass)[]
  field (int | string | OtherClass)?

  field map<string, string>
  field map<string, OtherClass>
  field map<string, map<string, string>>
}

function CallGpt(input: string) -> string[] {
  client GptProvider
  prompt #"
    Do something with {{ input }}

    {{ ctx.output_format }}
  "#
}

test CallGptTest {
  functions [CallGpt]
  args {
    input "hello"
  }
}
