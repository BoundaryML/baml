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
    "string key" "value with spaces"
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
    // array of objects with nested config blocks (no colons)
    tools [
      {
        server_label "parallel_web_search"
        server_url "https://example.com"
        headers {
          "x-api-key" myapikey
          authorization "Bearer token"
        }
        type "mcp"
      }
      {
        another_tool "value"
      }
    ]
    block_string #"
      hello there my friend
      {# a comment in a prompt #}
    "#
  }
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- semantic_tokens
// config_dictionary.baml:1:1 (keyword) len=6 "client"
// config_dictionary.baml:1:7 (operator) len=1 "<"
// config_dictionary.baml:1:8 (type) len=3 "llm"
// config_dictionary.baml:1:11 (operator) len=1 ">"
// config_dictionary.baml:1:13 (struct) [declaration] len=8 "MyClient"
// config_dictionary.baml:2:3 (property) len=8 "provider"
// config_dictionary.baml:3:3 (comment) len=18 "// dictionary test"
// config_dictionary.baml:4:3 (property) len=7 "options"
// config_dictionary.baml:5:5 (comment) len=12 "// no commas"
// config_dictionary.baml:6:5 (property) len=10 "max_tokens"
// config_dictionary.baml:6:16 (number) len=3 "100"
// config_dictionary.baml:7:5 (property) len=30 "array_key_with_quoted_unquoted"
// config_dictionary.baml:7:38 (string) len=5 "\"abc\""
// config_dictionary.baml:7:45 (string) len=5 "\"abc\""
// config_dictionary.baml:7:52 (string) len=5 "\"ccc\""
// config_dictionary.baml:8:5 (comment) len=21 "// separate by commas"
// config_dictionary.baml:9:5 (property) len=4 "key2"
// config_dictionary.baml:9:10 (string) len=19 "\"some random value\""
// config_dictionary.baml:9:31 (property) len=5 "hello"
// config_dictionary.baml:9:37 (string) len=7 "\"world\""
// config_dictionary.baml:9:46 (property) len=5 "thing"
// config_dictionary.baml:9:52 (string) len=7 "\"hello\""
// config_dictionary.baml:10:6 (property) len=12 "block_string"
// config_dictionary.baml:10:19 (string) len=1 "#"
// config_dictionary.baml:10:20 (string) len=1 "\""
// config_dictionary.baml:10:21 (string) len=5 "hello"
// config_dictionary.baml:10:27 (string) len=5 "there"
// config_dictionary.baml:10:33 (string) len=2 "my"
// config_dictionary.baml:10:36 (string) len=5 "frien"
// config_dictionary.baml:11:17 (string) len=1 "d"
// config_dictionary.baml:11:18 (string) len=1 "\""
// config_dictionary.baml:11:19 (string) len=1 "#"
// config_dictionary.baml:12:5 (property) len=17 "inline_raw_string"
// config_dictionary.baml:12:23 (string) len=1 "#"
// config_dictionary.baml:12:24 (string) len=1 "\""
// config_dictionary.baml:12:25 (string) len=6 "inline"
// config_dictionary.baml:12:32 (string) len=1 "\""
// config_dictionary.baml:12:33 (string) len=3 "raw"
// config_dictionary.baml:12:36 (string) len=1 "\""
// config_dictionary.baml:12:38 (string) len=6 "string"
// config_dictionary.baml:12:44 (string) len=1 ","
// config_dictionary.baml:12:45 (string) len=1 ","
// config_dictionary.baml:12:46 (string) len=1 "\""
// config_dictionary.baml:12:47 (string) len=1 "#"
// config_dictionary.baml:13:5 (string) len=12 "\"string key\""
// config_dictionary.baml:13:18 (string) len=19 "\"value with spaces\""
// config_dictionary.baml:14:5 (property) len=12 "booleanValue"
// config_dictionary.baml:14:18 (keyword) len=4 "true"
// config_dictionary.baml:15:5 (property) len=9 "stringKey"
// config_dictionary.baml:15:15 (string) len=6 "\"true\""
// config_dictionary.baml:16:5 (property) len=6 "array1"
// config_dictionary.baml:17:7 (string) len=5 "\"one\""
// config_dictionary.baml:18:7 (string) len=5 "\"two\""
// config_dictionary.baml:20:5 (comment) len=32 "// a comment and new empty_lines"
// config_dictionary.baml:22:5 (property) len=6 "array2"
// config_dictionary.baml:22:14 (string) len=5 "\"one\""
// config_dictionary.baml:22:21 (string) len=5 "\"two\""
// config_dictionary.baml:23:5 (property) len=13 "array3Numbers"
// config_dictionary.baml:23:21 (number) len=1 "1"
// config_dictionary.baml:23:24 (number) len=1 "2"
// config_dictionary.baml:23:27 (number) len=1 "3"
// config_dictionary.baml:24:5 (property) len=9 "nestedKey"
// config_dictionary.baml:25:7 (property) len=3 "key"
// config_dictionary.baml:25:11 (string) len=7 "\"value\""
// config_dictionary.baml:26:7 (property) len=4 "key2"
// config_dictionary.baml:26:12 (string) len=8 "\"value2\""
// config_dictionary.baml:27:7 (property) len=11 "nestedArray"
// config_dictionary.baml:27:21 (string) len=5 "\"yes\""
// config_dictionary.baml:27:28 (string) len=7 "\"queen\""
// config_dictionary.baml:28:7 (property) len=6 "number"
// config_dictionary.baml:28:14 (number) len=2 "10"
// config_dictionary.baml:30:5 (property) len=28 "oneMoreValueToTestBrokenness"
// config_dictionary.baml:30:34 (string) len=7 "\"value\""
// config_dictionary.baml:31:5 (property) len=9 "myInteger"
// config_dictionary.baml:31:15 (number) len=3 "100"
// config_dictionary.baml:32:5 (comment) len=57 "// array of objects with nested config blocks (no colons)"
// config_dictionary.baml:33:5 (property) len=5 "tools"
// config_dictionary.baml:35:9 (property) len=12 "server_label"
// config_dictionary.baml:35:22 (string) len=21 "\"parallel_web_search\""
// config_dictionary.baml:36:9 (property) len=10 "server_url"
// config_dictionary.baml:36:20 (string) len=21 "\"https://example.com\""
// config_dictionary.baml:37:9 (property) len=7 "headers"
// config_dictionary.baml:38:11 (string) len=11 "\"x-api-key\""
// config_dictionary.baml:39:11 (property) len=13 "authorization"
// config_dictionary.baml:39:25 (string) len=14 "\"Bearer token\""
// config_dictionary.baml:41:9 (property) len=4 "type"
// config_dictionary.baml:41:14 (string) len=5 "\"mcp\""
// config_dictionary.baml:44:9 (property) len=12 "another_tool"
// config_dictionary.baml:44:22 (string) len=7 "\"value\""
// config_dictionary.baml:47:5 (property) len=12 "block_string"
// config_dictionary.baml:47:18 (string) len=1 "#"
// config_dictionary.baml:47:19 (string) len=1 "\""
// config_dictionary.baml:48:7 (string) len=5 "hello"
// config_dictionary.baml:48:13 (string) len=5 "there"
// config_dictionary.baml:48:19 (string) len=2 "my"
// config_dictionary.baml:48:22 (string) len=6 "friend"
// config_dictionary.baml:49:7 (string) len=1 "{"
// config_dictionary.baml:49:8 (string) len=1 "#"
// config_dictionary.baml:49:10 (string) len=1 "a"
// config_dictionary.baml:49:12 (string) len=7 "comment"
// config_dictionary.baml:49:20 (string) len=2 "in"
// config_dictionary.baml:49:23 (string) len=1 "a"
// config_dictionary.baml:49:25 (string) len=6 "prompt"
// config_dictionary.baml:49:32 (string) len=1 "#"
// config_dictionary.baml:49:33 (string) len=1 "}"
// config_dictionary.baml:50:5 (string) len=1 "\""
// config_dictionary.baml:50:6 (string) len=1 "#"
