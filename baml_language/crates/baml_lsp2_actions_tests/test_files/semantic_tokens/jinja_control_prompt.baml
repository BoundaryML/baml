class Message {
  role "user" | "assistant" | string
  message string
}

function Bot(convo: Message[]) -> string {
  client "openai/gpt-4o"
  prompt #"
    You are a helpful assistant.
    {{ ctx.output_format }}

    {% for m in convo %}
    {{ _.role(m.role) }}
    {{ m.message }}
    {% endfor %}
  "#
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
// jinja_control_prompt.baml:7:10 (string) len=15 "\"openai/gpt-4o\""
// jinja_control_prompt.baml:8:3 (property) [declaration] len=6 "prompt"
// jinja_control_prompt.baml:8:10 (string) len=1 "#"
// jinja_control_prompt.baml:8:11 (string) len=1 "\""
// jinja_control_prompt.baml:9:5 (string) len=3 "You"
// jinja_control_prompt.baml:9:9 (string) len=3 "are"
// jinja_control_prompt.baml:9:13 (string) len=1 "a"
// jinja_control_prompt.baml:9:15 (string) len=7 "helpful"
// jinja_control_prompt.baml:9:23 (string) len=9 "assistant"
// jinja_control_prompt.baml:9:32 (string) len=1 "."
// jinja_control_prompt.baml:10:5 (string) len=1 "{"
// jinja_control_prompt.baml:10:6 (string) len=1 "{"
// jinja_control_prompt.baml:10:8 (string) len=3 "ctx"
// jinja_control_prompt.baml:10:11 (string) len=1 "."
// jinja_control_prompt.baml:10:12 (string) len=13 "output_format"
// jinja_control_prompt.baml:10:26 (string) len=1 "}"
// jinja_control_prompt.baml:10:27 (string) len=1 "}"
// jinja_control_prompt.baml:12:5 (string) len=1 "{"
// jinja_control_prompt.baml:12:6 (string) len=1 "%"
// jinja_control_prompt.baml:12:8 (string) len=3 "for"
// jinja_control_prompt.baml:12:12 (string) len=1 "m"
// jinja_control_prompt.baml:12:14 (string) len=2 "in"
// jinja_control_prompt.baml:12:17 (string) len=5 "convo"
// jinja_control_prompt.baml:12:23 (string) len=1 "%"
// jinja_control_prompt.baml:12:24 (string) len=1 "}"
// jinja_control_prompt.baml:13:5 (string) len=1 "{"
// jinja_control_prompt.baml:13:6 (string) len=1 "{"
// jinja_control_prompt.baml:13:8 (string) len=1 "_"
// jinja_control_prompt.baml:13:9 (string) len=1 "."
// jinja_control_prompt.baml:13:10 (string) len=4 "role"
// jinja_control_prompt.baml:13:14 (string) len=1 "("
// jinja_control_prompt.baml:13:15 (string) len=1 "m"
// jinja_control_prompt.baml:13:16 (string) len=1 "."
// jinja_control_prompt.baml:13:17 (string) len=4 "role"
// jinja_control_prompt.baml:13:21 (string) len=1 ")"
// jinja_control_prompt.baml:13:23 (string) len=1 "}"
// jinja_control_prompt.baml:13:24 (string) len=1 "}"
// jinja_control_prompt.baml:14:5 (string) len=1 "{"
// jinja_control_prompt.baml:14:6 (string) len=1 "{"
// jinja_control_prompt.baml:14:8 (string) len=1 "m"
// jinja_control_prompt.baml:14:9 (string) len=1 "."
// jinja_control_prompt.baml:14:10 (string) len=7 "message"
// jinja_control_prompt.baml:14:18 (string) len=1 "}"
// jinja_control_prompt.baml:14:19 (string) len=1 "}"
// jinja_control_prompt.baml:15:5 (string) len=1 "{"
// jinja_control_prompt.baml:15:6 (string) len=1 "%"
// jinja_control_prompt.baml:15:8 (string) len=6 "endfor"
// jinja_control_prompt.baml:15:15 (string) len=1 "%"
// jinja_control_prompt.baml:15:16 (string) len=1 "}"
// jinja_control_prompt.baml:16:3 (string) len=1 "\""
// jinja_control_prompt.baml:16:4 (string) len=1 "#"
