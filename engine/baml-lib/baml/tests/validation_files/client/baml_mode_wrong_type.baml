// Test that baml_mode with wrong type (not a string) is rejected

client<llm> BamlModeWrongType1 {
  provider openai
  options {
    model "gpt-4o"
    baml_mode 123  // Number instead of string
  }
}

client<llm> BamlModeWrongType2 {
  provider openai
  options {
    model "gpt-4o"
    baml_mode true  // Boolean instead of string
  }
}

client<llm> BamlModeWrongType3 {
  provider openai
  options {
    model "gpt-4o"
    baml_mode ["text"]  // Array instead of string
  }
}

client<llm> BamlModeWrongType4 {
  provider openai
  options {
    model "gpt-4o"
    baml_mode { mode "text" }  // Object instead of string
  }
}

// error: baml_mode must be a string. Got: number
//   -->  client/baml_mode_wrong_type.baml:7
//    | 
//  6 |     model "gpt-4o"
//  7 |     baml_mode 123  // Number instead of string
//    | 
// error: baml_mode must be a string. Got: bool
//   -->  client/baml_mode_wrong_type.baml:15
//    | 
// 14 |     model "gpt-4o"
// 15 |     baml_mode true  // Boolean instead of string
//    | 
// error: baml_mode must be a string. Got: string[]
//   -->  client/baml_mode_wrong_type.baml:23
//    | 
// 22 |     model "gpt-4o"
// 23 |     baml_mode ["text"]  // Array instead of string
//    | 
// error: baml_mode must be a string. Got: {
// mode: string
// }
//   -->  client/baml_mode_wrong_type.baml:31
//    | 
// 30 |     model "gpt-4o"
// 31 |     baml_mode { mode "text" }  // Object instead of string
//    | 
