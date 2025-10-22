client<llm> Hello {
  provider baml-openai-chat
  options {
    thing hello'world
    banned @helloworld
    banned2 #helloworld
    banned3 hello(world)
  }
}

// error: Error validating: This line is invalid. It does not start with any known Baml schema keyword.
//   -->  strings/unquoted_strings.baml:1
//    | 
//    | 
//  1 | client<llm> Hello {
//  2 |   provider baml-openai-chat
//    | 
