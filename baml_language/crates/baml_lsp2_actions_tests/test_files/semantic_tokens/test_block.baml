class Input {
  text string
}

client<llm> GPT4 {
  provider openai
  options {
    model "gpt-4"
  }
}

function Summarize(input: Input) -> string {
  client GPT4
  prompt #"
    Summarize: {{ input.text }}
  "#
}

test BasicTest {
  functions [Summarize]
  args {
    input {
      text "Hello world"
    }
  }
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- semantic_tokens
// test_block.baml:1:1 (keyword) len=5 "class"
// test_block.baml:1:7 (class) len=5 "Input"
// test_block.baml:2:3 (property) len=4 "text"
// test_block.baml:2:8 (type) len=6 "string"
// test_block.baml:5:1 (keyword) len=6 "client"
// test_block.baml:5:7 (operator) len=1 "<"
// test_block.baml:5:8 (type) len=3 "llm"
// test_block.baml:5:11 (operator) len=1 ">"
// test_block.baml:5:13 (struct) len=4 "GPT4"
// test_block.baml:6:3 (property) len=8 "provider"
// test_block.baml:7:3 (property) len=7 "options"
// test_block.baml:8:5 (property) len=5 "model"
// test_block.baml:8:11 (string) len=1 "\""
// test_block.baml:8:12 (string) len=5 "gpt-4"
// test_block.baml:8:17 (string) len=1 "\""
// test_block.baml:12:1 (keyword) len=8 "function"
// test_block.baml:12:10 (function) len=9 "Summarize"
// test_block.baml:12:20 (parameter) len=5 "input"
// test_block.baml:12:27 (class) len=5 "Input"
// test_block.baml:12:37 (type) len=6 "string"
// test_block.baml:13:3 (property) len=6 "client"
// test_block.baml:14:3 (property) len=6 "prompt"
// test_block.baml:14:10 (string) len=1 "#"
// test_block.baml:14:11 (string) len=1 "\""
// test_block.baml:15:5 (string) len=9 "Summarize"
// test_block.baml:15:14 (string) len=1 ":"
// test_block.baml:15:16 (string) len=1 "{"
// test_block.baml:15:17 (string) len=1 "{"
// test_block.baml:15:19 (string) len=5 "input"
// test_block.baml:15:24 (string) len=1 "."
// test_block.baml:15:25 (string) len=4 "text"
// test_block.baml:15:30 (string) len=1 "}"
// test_block.baml:15:31 (string) len=1 "}"
// test_block.baml:16:3 (string) len=1 "\""
// test_block.baml:16:4 (string) len=1 "#"
// test_block.baml:19:1 (keyword) len=4 "test"
// test_block.baml:19:6 (struct) len=9 "BasicTest"
// test_block.baml:20:3 (property) len=9 "functions"
// test_block.baml:21:3 (property) len=4 "args"
// test_block.baml:22:5 (property) len=5 "input"
// test_block.baml:23:7 (property) len=4 "text"
// test_block.baml:23:12 (string) len=1 "\""
// test_block.baml:23:13 (string) len=5 "Hello"
// test_block.baml:23:19 (string) len=5 "world"
// test_block.baml:23:24 (string) len=1 "\""
