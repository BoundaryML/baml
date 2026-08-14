class Email {
  subject string
  body string
}

enum Category {
  Spam
  Ham
}

client GPT4 = openai.ResponsesClient.new(model = "gpt-4");

function ClassifyEmail(email: Email) -> Category {
  client GPT4
  prompt `
    Classify this email:
    Subject: ${email.subject}
    Body: ${email.body}

    ${ctx.output_format}
  `
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- semantic_tokens
// llm_function.baml:1:1 (keyword) len=5 "class"
// llm_function.baml:1:7 (class) [declaration] len=5 "Email"
// llm_function.baml:2:3 (property) [declaration] len=7 "subject"
// llm_function.baml:2:11 (type) [defaultLibrary] len=6 "string"
// llm_function.baml:3:3 (property) [declaration] len=4 "body"
// llm_function.baml:3:8 (type) [defaultLibrary] len=6 "string"
// llm_function.baml:6:1 (keyword) len=4 "enum"
// llm_function.baml:6:6 (enum) [declaration] len=8 "Category"
// llm_function.baml:7:3 (enumMember) [declaration] len=4 "Spam"
// llm_function.baml:8:3 (enumMember) [declaration] len=3 "Ham"
// llm_function.baml:11:1 (keyword) len=6 "client"
// llm_function.baml:11:13 (operator) len=1 "="
// llm_function.baml:11:15 (namespace) [defaultLibrary] len=6 "openai"
// llm_function.baml:11:21 (namespace) [defaultLibrary] len=1 "."
// llm_function.baml:11:22 (class) [defaultLibrary] len=15 "ResponsesClient"
// llm_function.baml:11:38 (method) len=3 "new"
// llm_function.baml:11:42 (parameter) len=5 "model"
// llm_function.baml:11:48 (operator) len=1 "="
// llm_function.baml:11:50 (string) len=7 "\"gpt-4\""
// llm_function.baml:13:1 (keyword) len=8 "function"
// llm_function.baml:13:10 (function) [declaration] len=13 "ClassifyEmail"
// llm_function.baml:13:24 (parameter) [declaration] len=5 "email"
// llm_function.baml:13:31 (class) len=5 "Email"
// llm_function.baml:13:41 (enum) len=8 "Category"
// llm_function.baml:14:3 (property) len=6 "client"
// llm_function.baml:14:10 (variable) len=4 "GPT4"
// llm_function.baml:15:3 (property) [declaration] len=6 "prompt"
// llm_function.baml:15:10 (string) len=1 "`"
// llm_function.baml:16:5 (string) len=8 "Classify"
// llm_function.baml:16:14 (string) len=4 "this"
// llm_function.baml:16:19 (string) len=5 "email"
// llm_function.baml:16:24 (string) len=1 ":"
// llm_function.baml:17:5 (string) len=7 "Subject"
// llm_function.baml:17:12 (string) len=1 ":"
// llm_function.baml:17:16 (parameter) len=5 "email"
// llm_function.baml:17:22 (property) len=7 "subject"
// llm_function.baml:18:5 (string) len=4 "Body"
// llm_function.baml:18:9 (string) len=1 ":"
// llm_function.baml:18:13 (parameter) len=5 "email"
// llm_function.baml:18:19 (property) len=4 "body"
// llm_function.baml:20:11 (property) len=13 "output_format"
// llm_function.baml:21:3 (string) len=1 "`"
