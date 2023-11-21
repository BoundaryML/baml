class Bar {
  hello string
}

function Foo {
  input (arg_name: Bar, arg_2: Bar)
  output string
}

function ExtractAllergies {
  input (hello: string, world: string)
  output Allergies[]
}
