client<llm> Hello {
  provider baml-openai-chat
  options {
    thing hello'world
    banned @helloworld
    banned2 #helloworld
    banned3 hello(world)
  }
}

// error: Error validating: This line is not a valid field or attribute definition. A valid property may look like: 'myProperty "some value"' for example, with no colons.
//   -->  strings/unquoted_strings.baml:3
//    | 
//  2 |   provider baml-openai-chat
//  3 |   options {
//  4 |     thing hello'world
//    | 
// error: Error validating: This line is not a valid field or attribute definition. A valid property may look like: 'myProperty "some value"' for example, with no colons.
//   -->  strings/unquoted_strings.baml:4
//    | 
//  3 |   options {
//  4 |     thing hello'world
//  5 |     banned @helloworld
//    | 
// error: Error validating: This line is not a valid field or attribute definition. A valid property may look like: 'myProperty "some value"' for example, with no colons.
//   -->  strings/unquoted_strings.baml:6
//    | 
//  5 |     banned @helloworld
//  6 |     banned2 #helloworld
//  7 |     banned3 hello(world)
//    | 
// error: Error validating: This line is not a valid field or attribute definition. A valid property may look like: 'myProperty "some value"' for example, with no colons.
//   -->  strings/unquoted_strings.baml:7
//    | 
//  6 |     banned2 #helloworld
//  7 |     banned3 hello(world)
//  8 |   }
//    | 
// error: Error validating: This line is invalid. It does not start with any known Baml schema keyword.
//   -->  strings/unquoted_strings.baml:9
//    | 
//  8 |   }
//  9 | }
// 10 | 
//    | 
