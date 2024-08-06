client<llm> Hello {
  provider openai
  options {
    thing helloworld
    banned @helloworld
    // banned2 #helloworld
    banned3 hello(world)
  }
}

// error: Error validating Client "Hello": This field declaration is invalid. It is either missing a name or a type.
//   -->  strings/unquoted_strings.baml:3
//    | 
//  2 |   provider openai
//  3 |   options {
//    | 
// error: Error validating: This line is not a valid field or attribute definition.
//   -->  strings/unquoted_strings.baml:3
//    | 
//  2 |   provider openai
//  3 |   options {
//  4 |     thing helloworld
//    | 
// error: Error validating Client "Hello": This field declaration is invalid. It is either missing a name or a type.
//   -->  strings/unquoted_strings.baml:5
//    | 
//  4 |     thing helloworld
//  5 |     banned @helloworld
//    | 
// error: Error validating: This line is not a valid field or attribute definition.
//   -->  strings/unquoted_strings.baml:7
//    | 
//  6 |     // banned2 #helloworld
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
// error: Error validating: Unknown field `thing` in client
//   -->  strings/unquoted_strings.baml:4
//    | 
//  3 |   options {
//  4 |     thing helloworld
//    | 
// error: Error validating: Unknown field `banned3` in client
//   -->  strings/unquoted_strings.baml:7
//    | 
//  6 |     // banned2 #helloworld
//  7 |     banned3 hello(world)
//    | 
