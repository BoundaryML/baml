client<llm> MyClient {
  provider baml-openai-chat
  // dictionary test
  options {
    // no commas
    max_tokens 100
    array_key_with_quoted_unquoted [ "abc", "abc", "ccc" ]
    // separate by commas
    key2 "some random value", hello "world", thing "hello"
     block_string #"hello there my frien
                d"#
    inline_raw_string #"inline "raw" string,,"#
    keyStringNoSpacesWorksWithoutQuotes NoQuotes!!123
    unquoted_string_whitespace hello world
    "string key" "value with spaces"
    filePathString ../this-is-also-a-string
    booleanValue true
    stringKey "true"
    array1 [
      "one"
      "two"
    ]
    // a comment and new empty_lines

    array2 [ "one", "two" ]
    array3Numbers [ 1, 2, 3 ]
    nestedKey {
      key "value"
      key2 "value2"
      nestedArray [ "yes", "queen"]
      number 10
    }
    oneMoreValueToTestBrokenness "value"
    myInteger 100
    block_string #"
      hello there my friend
      #{//a comment in a prompt}
    "#
  }
}

//----
//- diagnostics
// Error: Expected config key, found Comma
//    ╭─[ 0:7:43 ]
//    │
//  7 │     array_key_with_quoted_unquoted [ "abc", "abc", "ccc" ]
//    │                                           ┬  
//    │                                           ╰── Expected config key, found Comma
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected config key, found Quote
//    ╭─[ 0:7:45 ]
//    │
//  7 │     array_key_with_quoted_unquoted [ "abc", "abc", "ccc" ]
//    │                                             ┬  
//    │                                             ╰── Expected config key, found Quote
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected config key, found Comma
//    ╭─[ 0:9:29 ]
//    │
//  9 │     key2 "some random value", hello "world", thing "hello"
//    │                             ┬  
//    │                             ╰── Expected config key, found Comma
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected config key, found Comma
//    ╭─[ 0:9:44 ]
//    │
//  9 │     key2 "some random value", hello "world", thing "hello"
//    │                                            ┬  
//    │                                            ╰── Expected config key, found Comma
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected config key, found Quote
//     ╭─[ 0:15:5 ]
//     │
//  15 │     "string key" "value with spaces"
//     │     ┬  
//     │     ╰── Expected config key, found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected config key, found Quote
//     ╭─[ 0:20:7 ]
//     │
//  20 │       "one"
//     │       ┬  
//     │       ╰── Expected config key, found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected config key, found Hash
//     ╭─[ 0:37:7 ]
//     │
//  37 │       #{//a comment in a prompt}
//     │       ┬  
//     │       ╰── Expected config key, found Hash
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected config key, found LBrace
//     ╭─[ 0:37:8 ]
//     │
//  37 │       #{//a comment in a prompt}
//     │        ┬  
//     │        ╰── Expected config key, found LBrace
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected config key, found Quote
//     ╭─[ 0:38:5 ]
//     │
//  38 │     "#
//     │     ┬  
//     │     ╰── Expected config key, found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected config key, found Hash
//     ╭─[ 0:38:6 ]
//     │
//  38 │     "#
//     │      ┬  
//     │      ╰── Expected config key, found Hash
//     │ 
//     │ Note: Error code: E0010
// ────╯
