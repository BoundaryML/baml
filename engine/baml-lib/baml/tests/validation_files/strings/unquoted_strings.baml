
client<llm> Hello {
  provider baml-openai-chat
  options {
    thing hello'world
    banned @helloworld
    banned2 #helloworld
    banned3 hello(world)
  }
}

// error: Error validating: This line is not a valid field or attribute definition.
//   -->  strings/unquoted_strings.baml:4
//    | 
//  3 |   provider baml-openai-chat
//  4 |   options {
//  5 |     thing hello'world
//    | 
// error: Error validating: This line is not a valid field or attribute definition.
//   -->  strings/unquoted_strings.baml:5
//    | 
//  4 |   options {
//  5 |     thing hello'world
//  6 |     banned @helloworld
//    | 
// error: Error validating: This line is not a valid field or attribute definition.
//   -->  strings/unquoted_strings.baml:7
//    | 
//  6 |     banned @helloworld
//  7 |     banned2 #helloworld
//  8 |     banned3 hello(world)
//    | 
// error: Error validating: This line is not a valid field or attribute definition.
//   -->  strings/unquoted_strings.baml:8
//    | 
//  7 |     banned2 #helloworld
//  8 |     banned3 hello(world)
//  9 |   }
//    | 
// error: Error validating: This line is invalid. It does not start with any known Baml schema keyword.
//   -->  strings/unquoted_strings.baml:10
//    | 
//  9 |   }
// 10 | }
// 11 | // error: Error validating: This line is not a valid field or attribute definition.
//    | 
