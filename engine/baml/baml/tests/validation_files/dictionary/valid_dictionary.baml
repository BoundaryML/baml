client<llm> MyClient {
  provider openai
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