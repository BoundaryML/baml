class Input {
  text string
}

client GPT4 = openai.OpenAiClient.new(model = "gpt-4");

function Summarize(input: Input) -> string {
  client: GPT4
  prompt: `
    Summarize: ${input.text}
  `
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
// test_block.baml:1:7 (class) [declaration] len=5 "Input"
// test_block.baml:2:3 (property) [declaration] len=4 "text"
// test_block.baml:2:8 (type) [defaultLibrary] len=6 "string"
// test_block.baml:5:1 (keyword) len=6 "client"
// test_block.baml:5:13 (operator) len=1 "="
// test_block.baml:5:15 (namespace) [defaultLibrary] len=6 "openai"
// test_block.baml:5:21 (namespace) [defaultLibrary] len=1 "."
// test_block.baml:5:22 (class) [defaultLibrary] len=12 "OpenAiClient"
// test_block.baml:5:35 (method) len=3 "new"
// test_block.baml:5:39 (parameter) len=5 "model"
// test_block.baml:5:45 (operator) len=1 "="
// test_block.baml:5:47 (string) len=7 "\"gpt-4\""
// test_block.baml:7:1 (keyword) len=8 "function"
// test_block.baml:7:10 (function) [declaration] len=9 "Summarize"
// test_block.baml:7:20 (parameter) [declaration] len=5 "input"
// test_block.baml:7:27 (class) len=5 "Input"
// test_block.baml:7:37 (type) [defaultLibrary] len=6 "string"
// test_block.baml:8:3 (property) len=6 "client"
// test_block.baml:8:11 (variable) len=4 "GPT4"
// test_block.baml:9:3 (property) [declaration] len=6 "prompt"
// test_block.baml:9:11 (string) len=1 "`"
// test_block.baml:10:5 (string) len=9 "Summarize"
// test_block.baml:10:14 (string) len=1 ":"
// test_block.baml:10:18 (parameter) len=5 "input"
// test_block.baml:10:24 (property) len=4 "text"
// test_block.baml:11:3 (string) len=1 "`"
// test_block.baml:14:1 (keyword) len=4 "test"
// test_block.baml:14:6 (struct) [declaration] len=9 "BasicTest"
// test_block.baml:15:3 (property) len=9 "functions"
// test_block.baml:16:3 (property) len=4 "args"
// test_block.baml:17:5 (property) len=5 "input"
// test_block.baml:18:7 (property) len=4 "text"
// test_block.baml:18:12 (string) len=13 "\"Hello world\""
