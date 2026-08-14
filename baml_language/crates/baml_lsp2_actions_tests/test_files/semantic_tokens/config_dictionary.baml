// The colon-free config dictionary surface now lives in test args blocks.
function DictionaryProbe(payload: string) -> string {
  client: "openai/gpt-4o-mini"
  prompt: `Use ${payload} ${ctx.output_format}`
}

test DictionaryTest {
  functions [DictionaryProbe]
  // dictionary test
  args {
    payload "hello"
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
  }
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- semantic_tokens
// config_dictionary.baml:1:1 (comment) len=74 "// The colon-free config dictionary surface now lives in test args blocks."
// config_dictionary.baml:2:1 (keyword) len=8 "function"
// config_dictionary.baml:2:10 (function) [declaration] len=15 "DictionaryProbe"
// config_dictionary.baml:2:26 (parameter) [declaration] len=7 "payload"
// config_dictionary.baml:2:35 (type) [defaultLibrary] len=6 "string"
// config_dictionary.baml:2:46 (type) [defaultLibrary] len=6 "string"
// config_dictionary.baml:3:3 (property) len=6 "client"
// config_dictionary.baml:3:11 (string) len=20 "\"openai/gpt-4o-mini\""
// config_dictionary.baml:4:3 (property) [declaration] len=6 "prompt"
// config_dictionary.baml:4:11 (string) len=1 "`"
// config_dictionary.baml:4:12 (string) len=3 "Use"
// config_dictionary.baml:4:18 (parameter) len=7 "payload"
// config_dictionary.baml:4:33 (property) len=13 "output_format"
// config_dictionary.baml:4:47 (string) len=1 "`"
// config_dictionary.baml:7:1 (keyword) len=4 "test"
// config_dictionary.baml:7:6 (struct) [declaration] len=14 "DictionaryTest"
// config_dictionary.baml:8:3 (property) len=9 "functions"
// config_dictionary.baml:9:3 (comment) len=18 "// dictionary test"
// config_dictionary.baml:10:3 (property) len=4 "args"
// config_dictionary.baml:11:5 (property) len=7 "payload"
// config_dictionary.baml:11:13 (string) len=7 "\"hello\""
// config_dictionary.baml:12:5 (comment) len=12 "// no commas"
// config_dictionary.baml:13:5 (property) len=10 "max_tokens"
// config_dictionary.baml:13:16 (number) len=3 "100"
// config_dictionary.baml:14:5 (property) len=30 "array_key_with_quoted_unquoted"
// config_dictionary.baml:14:38 (string) len=5 "\"abc\""
// config_dictionary.baml:14:45 (string) len=5 "\"abc\""
// config_dictionary.baml:14:52 (string) len=5 "\"ccc\""
// config_dictionary.baml:15:5 (comment) len=21 "// separate by commas"
// config_dictionary.baml:16:5 (property) len=4 "key2"
// config_dictionary.baml:16:10 (string) len=19 "\"some random value\""
// config_dictionary.baml:16:31 (property) len=5 "hello"
// config_dictionary.baml:16:37 (string) len=7 "\"world\""
// config_dictionary.baml:16:46 (property) len=5 "thing"
// config_dictionary.baml:16:52 (string) len=7 "\"hello\""
// config_dictionary.baml:17:6 (property) len=12 "block_string"
// config_dictionary.baml:17:19 (string) len=1 "#"
// config_dictionary.baml:17:20 (string) len=1 "\""
// config_dictionary.baml:17:21 (string) len=5 "hello"
// config_dictionary.baml:17:27 (string) len=5 "there"
// config_dictionary.baml:17:33 (string) len=2 "my"
// config_dictionary.baml:17:36 (string) len=5 "frien"
// config_dictionary.baml:18:17 (string) len=1 "d"
// config_dictionary.baml:18:18 (string) len=1 "\""
// config_dictionary.baml:18:19 (string) len=1 "#"
// config_dictionary.baml:19:5 (property) len=17 "inline_raw_string"
// config_dictionary.baml:19:23 (string) len=1 "#"
// config_dictionary.baml:19:24 (string) len=1 "\""
// config_dictionary.baml:19:25 (string) len=6 "inline"
// config_dictionary.baml:19:32 (string) len=1 "\""
// config_dictionary.baml:19:33 (string) len=3 "raw"
// config_dictionary.baml:19:36 (string) len=1 "\""
// config_dictionary.baml:19:38 (string) len=6 "string"
// config_dictionary.baml:19:44 (string) len=1 ","
// config_dictionary.baml:19:45 (string) len=1 ","
// config_dictionary.baml:19:46 (string) len=1 "\""
// config_dictionary.baml:19:47 (string) len=1 "#"
// config_dictionary.baml:20:5 (string) len=12 "\"string key\""
// config_dictionary.baml:20:18 (string) len=19 "\"value with spaces\""
// config_dictionary.baml:21:5 (property) len=12 "booleanValue"
// config_dictionary.baml:21:18 (boolean) len=4 "true"
// config_dictionary.baml:22:5 (property) len=9 "stringKey"
// config_dictionary.baml:22:15 (string) len=6 "\"true\""
// config_dictionary.baml:23:5 (property) len=6 "array1"
// config_dictionary.baml:24:7 (string) len=5 "\"one\""
// config_dictionary.baml:25:7 (string) len=5 "\"two\""
// config_dictionary.baml:27:5 (comment) len=32 "// a comment and new empty_lines"
// config_dictionary.baml:29:5 (property) len=6 "array2"
// config_dictionary.baml:29:14 (string) len=5 "\"one\""
// config_dictionary.baml:29:21 (string) len=5 "\"two\""
// config_dictionary.baml:30:5 (property) len=13 "array3Numbers"
// config_dictionary.baml:30:21 (number) len=1 "1"
// config_dictionary.baml:30:24 (number) len=1 "2"
// config_dictionary.baml:30:27 (number) len=1 "3"
// config_dictionary.baml:31:5 (property) len=9 "nestedKey"
// config_dictionary.baml:32:7 (property) len=3 "key"
// config_dictionary.baml:32:11 (string) len=7 "\"value\""
// config_dictionary.baml:33:7 (property) len=4 "key2"
// config_dictionary.baml:33:12 (string) len=8 "\"value2\""
// config_dictionary.baml:34:7 (property) len=11 "nestedArray"
// config_dictionary.baml:34:21 (string) len=5 "\"yes\""
// config_dictionary.baml:34:28 (string) len=7 "\"queen\""
// config_dictionary.baml:35:7 (property) len=6 "number"
// config_dictionary.baml:35:14 (number) len=2 "10"
// config_dictionary.baml:37:5 (property) len=28 "oneMoreValueToTestBrokenness"
// config_dictionary.baml:37:34 (string) len=7 "\"value\""
// config_dictionary.baml:38:5 (property) len=9 "myInteger"
// config_dictionary.baml:38:15 (number) len=3 "100"
// config_dictionary.baml:39:5 (comment) len=57 "// array of objects with nested config blocks (no colons)"
// config_dictionary.baml:40:5 (property) len=5 "tools"
// config_dictionary.baml:42:9 (property) len=12 "server_label"
// config_dictionary.baml:42:22 (string) len=21 "\"parallel_web_search\""
// config_dictionary.baml:43:9 (property) len=10 "server_url"
// config_dictionary.baml:43:20 (string) len=21 "\"https://example.com\""
// config_dictionary.baml:44:9 (property) len=7 "headers"
// config_dictionary.baml:45:11 (string) len=11 "\"x-api-key\""
// config_dictionary.baml:46:11 (property) len=13 "authorization"
// config_dictionary.baml:46:25 (string) len=14 "\"Bearer token\""
// config_dictionary.baml:48:9 (property) len=4 "type"
// config_dictionary.baml:48:14 (string) len=5 "\"mcp\""
// config_dictionary.baml:51:9 (property) len=12 "another_tool"
// config_dictionary.baml:51:22 (string) len=7 "\"value\""
