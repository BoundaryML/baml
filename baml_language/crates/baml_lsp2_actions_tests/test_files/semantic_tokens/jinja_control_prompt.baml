class Message {
  role "user" | "assistant" | string
  message string
}

function Bot(convo: Message[]) -> string {
  client: "openai/gpt-4o"
  prompt: `
    You are a helpful assistant.
    ${ctx.output_format()}

    Conversation so far:
    ${convo}
  `
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- semantic_tokens
// jinja_control_prompt.baml:1:1 (keyword) len=5 "class"
// jinja_control_prompt.baml:1:7 (class) [declaration] len=7 "Message"
// jinja_control_prompt.baml:2:3 (property) [declaration] len=4 "role"
// jinja_control_prompt.baml:2:8 (string) len=6 "\"user\""
// jinja_control_prompt.baml:2:15 (operator) len=1 "|"
// jinja_control_prompt.baml:2:17 (string) len=11 "\"assistant\""
// jinja_control_prompt.baml:2:29 (operator) len=1 "|"
// jinja_control_prompt.baml:2:31 (type) [defaultLibrary] len=6 "string"
// jinja_control_prompt.baml:3:3 (property) [declaration] len=7 "message"
// jinja_control_prompt.baml:3:11 (type) [defaultLibrary] len=6 "string"
// jinja_control_prompt.baml:6:1 (keyword) len=8 "function"
// jinja_control_prompt.baml:6:10 (function) [declaration] len=3 "Bot"
// jinja_control_prompt.baml:6:14 (parameter) [declaration] len=5 "convo"
// jinja_control_prompt.baml:6:21 (class) len=7 "Message"
// jinja_control_prompt.baml:6:35 (type) [defaultLibrary] len=6 "string"
// jinja_control_prompt.baml:7:3 (property) len=6 "client"
// jinja_control_prompt.baml:7:11 (string) len=15 "\"openai/gpt-4o\""
// jinja_control_prompt.baml:8:3 (property) [declaration] len=6 "prompt"
// jinja_control_prompt.baml:8:11 (string) len=1 "`"
// jinja_control_prompt.baml:9:5 (string) len=3 "You"
// jinja_control_prompt.baml:9:9 (string) len=3 "are"
// jinja_control_prompt.baml:9:13 (string) len=1 "a"
// jinja_control_prompt.baml:9:15 (string) len=7 "helpful"
// jinja_control_prompt.baml:9:23 (string) len=9 "assistant"
// jinja_control_prompt.baml:9:32 (string) len=1 "."
// jinja_control_prompt.baml:10:11 (method) len=13 "output_format"
// jinja_control_prompt.baml:12:5 (string) len=12 "Conversation"
// jinja_control_prompt.baml:12:18 (string) len=2 "so"
// jinja_control_prompt.baml:12:21 (string) len=3 "far"
// jinja_control_prompt.baml:12:24 (string) len=1 ":"
// jinja_control_prompt.baml:13:7 (parameter) len=5 "convo"
// jinja_control_prompt.baml:14:3 (string) len=1 "`"
