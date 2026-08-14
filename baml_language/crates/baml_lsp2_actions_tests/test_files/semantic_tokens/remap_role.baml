// Role remapping options are gone along with client<llm> blocks; the client is
// a constructed value and the prompt is instructions-only.
client MyClient = openai.OpenAiClient.new(model = "gpt-4o");

function TestRemap(input: string) -> string {
  client: MyClient
  prompt: `
    Test message: ${input}
  `
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- semantic_tokens
// remap_role.baml:1:1 (comment) len=79 "// Role remapping options are gone along with client<llm> blocks; the client is"
// remap_role.baml:2:1 (comment) len=59 "// a constructed value and the prompt is instructions-only."
// remap_role.baml:3:1 (keyword) len=6 "client"
// remap_role.baml:3:17 (operator) len=1 "="
// remap_role.baml:3:19 (namespace) [defaultLibrary] len=6 "openai"
// remap_role.baml:3:25 (namespace) [defaultLibrary] len=1 "."
// remap_role.baml:3:26 (class) [defaultLibrary] len=12 "OpenAiClient"
// remap_role.baml:3:39 (method) len=3 "new"
// remap_role.baml:3:43 (parameter) len=5 "model"
// remap_role.baml:3:49 (operator) len=1 "="
// remap_role.baml:3:51 (string) len=8 "\"gpt-4o\""
// remap_role.baml:5:1 (keyword) len=8 "function"
// remap_role.baml:5:10 (function) [declaration] len=9 "TestRemap"
// remap_role.baml:5:20 (parameter) [declaration] len=5 "input"
// remap_role.baml:5:27 (type) [defaultLibrary] len=6 "string"
// remap_role.baml:5:38 (type) [defaultLibrary] len=6 "string"
// remap_role.baml:6:3 (property) len=6 "client"
// remap_role.baml:6:11 (variable) len=8 "MyClient"
// remap_role.baml:7:3 (property) [declaration] len=6 "prompt"
// remap_role.baml:7:11 (string) len=1 "`"
// remap_role.baml:8:5 (string) len=4 "Test"
// remap_role.baml:8:10 (string) len=7 "message"
// remap_role.baml:8:17 (string) len=1 ":"
// remap_role.baml:8:21 (parameter) len=5 "input"
// remap_role.baml:9:3 (string) len=1 "`"
