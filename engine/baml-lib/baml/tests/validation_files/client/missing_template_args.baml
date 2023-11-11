client MyClient {
  provider baml-openai-chat
}



// error: Error validating: Missing template for client. (did you forget <llm>)
//   -->  client/missing_template_args.baml:1
//    | 
//    | 
//  1 | client MyClient {
//  2 |   provider baml-openai-chat
//  3 | }
//    | 
