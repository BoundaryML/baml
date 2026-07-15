client<llm> MyClient {
  provider openai
  options {
    model "gpt-4o"
    allowed_roles ["user", "assistant", "system"]
    remap_roles {
      user "human"
      assistant "ai"
      system "instructions"
    }
    default_role "user"
  }
}

function TestRemap(input: string) -> string {
  client MyClient
  prompt #"
    {{ _.chat("user") }}
    Test message: {{ input }}
  "#
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- semantic_tokens
// remap_role.baml:1:1 (keyword) len=6 "client"
// remap_role.baml:1:7 (operator) len=1 "<"
// remap_role.baml:1:8 (type) len=3 "llm"
// remap_role.baml:1:11 (operator) len=1 ">"
// remap_role.baml:1:13 (struct) [declaration] len=8 "MyClient"
// remap_role.baml:2:3 (property) len=8 "provider"
// remap_role.baml:3:3 (property) len=7 "options"
// remap_role.baml:4:5 (property) len=5 "model"
// remap_role.baml:4:11 (string) len=8 "\"gpt-4o\""
// remap_role.baml:5:5 (property) len=13 "allowed_roles"
// remap_role.baml:5:20 (string) len=6 "\"user\""
// remap_role.baml:5:28 (string) len=11 "\"assistant\""
// remap_role.baml:5:41 (string) len=8 "\"system\""
// remap_role.baml:6:5 (property) len=11 "remap_roles"
// remap_role.baml:7:7 (property) len=4 "user"
// remap_role.baml:7:12 (string) len=7 "\"human\""
// remap_role.baml:8:7 (property) len=9 "assistant"
// remap_role.baml:8:17 (string) len=4 "\"ai\""
// remap_role.baml:9:7 (property) len=6 "system"
// remap_role.baml:9:14 (string) len=14 "\"instructions\""
// remap_role.baml:11:5 (property) len=12 "default_role"
// remap_role.baml:11:18 (string) len=6 "\"user\""
// remap_role.baml:15:1 (keyword) len=8 "function"
// remap_role.baml:15:10 (function) [declaration] len=9 "TestRemap"
// remap_role.baml:15:20 (parameter) [declaration] len=5 "input"
// remap_role.baml:15:27 (type) [defaultLibrary] len=6 "string"
// remap_role.baml:15:38 (type) [defaultLibrary] len=6 "string"
// remap_role.baml:16:3 (property) len=6 "client"
// remap_role.baml:17:3 (property) [declaration] len=6 "prompt"
// remap_role.baml:17:10 (string) len=1 "#"
// remap_role.baml:17:11 (string) len=1 "\""
// remap_role.baml:18:5 (string) len=1 "{"
// remap_role.baml:18:6 (string) len=1 "{"
// remap_role.baml:18:8 (string) len=1 "_"
// remap_role.baml:18:9 (string) len=1 "."
// remap_role.baml:18:10 (string) len=4 "chat"
// remap_role.baml:18:14 (string) len=1 "("
// remap_role.baml:18:15 (string) len=1 "\""
// remap_role.baml:18:16 (string) len=4 "user"
// remap_role.baml:18:20 (string) len=1 "\""
// remap_role.baml:18:21 (string) len=1 ")"
// remap_role.baml:18:23 (string) len=1 "}"
// remap_role.baml:18:24 (string) len=1 "}"
// remap_role.baml:19:5 (string) len=4 "Test"
// remap_role.baml:19:10 (string) len=7 "message"
// remap_role.baml:19:17 (string) len=1 ":"
// remap_role.baml:19:19 (string) len=1 "{"
// remap_role.baml:19:20 (string) len=1 "{"
// remap_role.baml:19:22 (string) len=5 "input"
// remap_role.baml:19:28 (string) len=1 "}"
// remap_role.baml:19:29 (string) len=1 "}"
// remap_role.baml:20:3 (string) len=1 "\""
// remap_role.baml:20:4 (string) len=1 "#"
