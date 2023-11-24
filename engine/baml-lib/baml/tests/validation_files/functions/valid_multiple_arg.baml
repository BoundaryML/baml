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

// error: Type `Allergies` does not exist. Did you mean one of these: `string`, `Bar`, `int`, `float`, `bool`?
//   -->  functions/valid_multiple_arg.baml:12
//    | 
// 11 |   input (hello: string, world: string)
// 12 |   output Allergies[]
//    | 
