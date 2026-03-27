class Email {
  subject string
  body string
}

enum Category {
  Spam
  Ham
}

client<llm> GPT4 {
  provider openai
  options {
    model "gpt-4"
  }
}

function ClassifyEmail(email: Email) -> Category {
  client GPT4
  prompt #"
    Classify this email:
    Subject: {{ email.subject }}
    Body: {{ email.body }}

    {{ ctx.output_format }}
  "#
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- semantic_tokens
// llm_function.baml:1:1 (keyword) len=5 "class"
// llm_function.baml:1:7 (class) len=5 "Email"
// llm_function.baml:2:3 (property) len=7 "subject"
// llm_function.baml:2:11 (type) len=6 "string"
// llm_function.baml:3:3 (property) len=4 "body"
// llm_function.baml:3:8 (type) len=6 "string"
// llm_function.baml:6:1 (keyword) len=4 "enum"
// llm_function.baml:6:6 (enum) len=8 "Category"
// llm_function.baml:7:3 (enumMember) len=4 "Spam"
// llm_function.baml:8:3 (enumMember) len=3 "Ham"
// llm_function.baml:11:1 (keyword) len=6 "client"
// llm_function.baml:11:7 (operator) len=1 "<"
// llm_function.baml:11:8 (type) len=3 "llm"
// llm_function.baml:11:11 (operator) len=1 ">"
// llm_function.baml:11:13 (struct) len=4 "GPT4"
// llm_function.baml:12:3 (property) len=8 "provider"
// llm_function.baml:13:3 (property) len=7 "options"
// llm_function.baml:14:5 (property) len=5 "model"
// llm_function.baml:14:11 (string) len=1 "\""
// llm_function.baml:14:12 (string) len=5 "gpt-4"
// llm_function.baml:14:17 (string) len=1 "\""
// llm_function.baml:18:1 (keyword) len=8 "function"
// llm_function.baml:18:10 (function) len=13 "ClassifyEmail"
// llm_function.baml:18:24 (parameter) len=5 "email"
// llm_function.baml:18:31 (class) len=5 "Email"
// llm_function.baml:18:41 (enum) len=8 "Category"
// llm_function.baml:19:3 (property) len=6 "client"
// llm_function.baml:20:3 (property) len=6 "prompt"
// llm_function.baml:20:10 (string) len=1 "#"
// llm_function.baml:20:11 (string) len=1 "\""
// llm_function.baml:21:5 (string) len=8 "Classify"
// llm_function.baml:21:14 (string) len=4 "this"
// llm_function.baml:21:19 (string) len=5 "email"
// llm_function.baml:21:24 (string) len=1 ":"
// llm_function.baml:22:5 (string) len=7 "Subject"
// llm_function.baml:22:12 (string) len=1 ":"
// llm_function.baml:22:14 (string) len=1 "{"
// llm_function.baml:22:15 (string) len=1 "{"
// llm_function.baml:22:17 (string) len=5 "email"
// llm_function.baml:22:22 (string) len=1 "."
// llm_function.baml:22:23 (string) len=7 "subject"
// llm_function.baml:22:31 (string) len=1 "}"
// llm_function.baml:22:32 (string) len=1 "}"
// llm_function.baml:23:5 (string) len=4 "Body"
// llm_function.baml:23:9 (string) len=1 ":"
// llm_function.baml:23:11 (string) len=1 "{"
// llm_function.baml:23:12 (string) len=1 "{"
// llm_function.baml:23:14 (string) len=5 "email"
// llm_function.baml:23:19 (string) len=1 "."
// llm_function.baml:23:20 (string) len=4 "body"
// llm_function.baml:23:25 (string) len=1 "}"
// llm_function.baml:23:26 (string) len=1 "}"
// llm_function.baml:25:5 (string) len=1 "{"
// llm_function.baml:25:6 (string) len=1 "{"
// llm_function.baml:25:8 (string) len=3 "ctx"
// llm_function.baml:25:11 (string) len=1 "."
// llm_function.baml:25:12 (string) len=13 "output_format"
// llm_function.baml:25:26 (string) len=1 "}"
// llm_function.baml:25:27 (string) len=1 "}"
// llm_function.baml:26:3 (string) len=1 "\""
// llm_function.baml:26:4 (string) len=1 "#"
