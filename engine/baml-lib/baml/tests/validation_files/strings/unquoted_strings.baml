client<llm> Hello {
  provider openai
  options {
    thing helloworld
    banned @helloworld
    // banned2 #helloworld
    // banned3 hello(world)
  }
}

// error: Error validating: This map entry is missing a valid value or has an incorrect syntax.
//   -->  strings/unquoted_strings.baml:5
//    | 
//  4 |     thing helloworld
//  5 |     banned @helloworld
//    | 
