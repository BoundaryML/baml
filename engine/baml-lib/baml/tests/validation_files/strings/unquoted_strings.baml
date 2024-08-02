client<llm> Hello {
  provider openai
  options {
    thing hello'world
    banned @helloworld
    banned2 #helloworld
    banned3 hello(world)
  }
}

// error: Error validating Client "Hello": expr as val!! This field declaration is invalid. It is either missing a name or a type.
//   -->  strings/unquoted_strings.baml:3
//    | 
//  2 |   provider openai
//  3 |   options {
//    | 
// error: Error validating: This line is invalid. It does not start with any known Baml schema keyword.
//   -->  strings/unquoted_strings.baml:9
//    | 
//  8 |   }
//  9 | }
// 10 | 
//    | 
