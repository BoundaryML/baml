client MyClient {
  provider baml-openai-chat
}


// error: Error validating: Missing template for client. (did you forget <llm>)
//   -->  /Users/vbv/repos/gloo-lang/engine/baml-lib/baml/tests/validation_files/client/missing_template_args.baml:1
//    | 
//    | 
//  1 | client MyClient {
//  2 |   provider baml-openai-chat
//  3 | }
//    | 
