client MyClient {
  provider openai
  
}


// error: Error validating: Missing template for client. (did you forget <llm>)
//   -->  /Users/aaronvillalpando/Projects/gloo-lang/engine/baml/baml/tests/validation_files/client/missing_template_args.baml:1
//    | 
//    | 
//  1 | client MyClient {
//  2 |   provider openai
//  3 |   
//  4 | }
//    | 
