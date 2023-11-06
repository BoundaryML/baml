client<l> MyClient2 {
  provider baml-openai-chat
}




// error: Error validating: Only LLM clients are supported. Use: client<llm>
//   -->  /Users/vbv/repos/gloo-lang/engine/baml-lib/baml/tests/validation_files/client/bad_template_args.baml:1
//    | 
//    | 
//  1 | client<l> MyClient2 {
//  2 |   provider baml-openai-chat
//  3 | }
//    | 
