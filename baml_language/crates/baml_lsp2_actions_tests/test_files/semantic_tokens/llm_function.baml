class Email {
    subject: string,
    body: string,
}

enum Category {
    Spam,
    Ham,
}

client GPT4 = openai.OpenAiClient.new(model = "gpt-4");

function ClassifyEmail(email: Email) -> Category {
    client: GPT4
    prompt: `
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
// llm_function.baml:2:5 (property) [declaration] len=7 "subject"
// llm_function.baml:2:14 (type) [defaultLibrary] len=6 "string"
// llm_function.baml:3:5 (property) [declaration] len=4 "body"
// llm_function.baml:3:11 (type) [defaultLibrary] len=6 "string"
// llm_function.baml:6:1 (keyword) len=4 "enum"
// llm_function.baml:6:6 (enum) [declaration] len=8 "Category"
// llm_function.baml:7:5 (enumMember) [declaration] len=4 "Spam"
// llm_function.baml:8:5 (enumMember) [declaration] len=3 "Ham"
// llm_function.baml:11:1 (keyword) len=6 "client"
// llm_function.baml:11:13 (operator) len=1 "="
// llm_function.baml:11:15 (namespace) [defaultLibrary] len=6 "openai"
// llm_function.baml:11:21 (namespace) [defaultLibrary] len=1 "."
// llm_function.baml:11:22 (class) [defaultLibrary] len=12 "OpenAiClient"
// llm_function.baml:11:35 (method) len=3 "new"
// llm_function.baml:11:39 (parameter) len=5 "model"
// llm_function.baml:11:45 (operator) len=1 "="
// llm_function.baml:11:47 (string) len=7 "\"gpt-4\""
// llm_function.baml:13:1 (keyword) len=8 "function"
// llm_function.baml:13:10 (function) [declaration] len=13 "ClassifyEmail"
// llm_function.baml:13:24 (parameter) [declaration] len=5 "email"
// llm_function.baml:13:31 (class) len=5 "Email"
// llm_function.baml:13:41 (enum) len=8 "Category"
// llm_function.baml:14:5 (property) len=6 "client"
// llm_function.baml:14:13 (variable) len=4 "GPT4"
// llm_function.baml:15:5 (property) [declaration] len=6 "prompt"
// llm_function.baml:15:13 (string) len=1 "`"
// llm_function.baml:16:9 (string) len=8 "Classify"
// llm_function.baml:16:18 (string) len=4 "this"
// llm_function.baml:16:23 (string) len=5 "email"
// llm_function.baml:16:28 (string) len=1 ":"
// llm_function.baml:17:9 (string) len=7 "Subject"
// llm_function.baml:17:16 (string) len=1 ":"
// llm_function.baml:17:20 (parameter) len=5 "email"
// llm_function.baml:17:26 (property) len=7 "subject"
// llm_function.baml:18:9 (string) len=4 "Body"
// llm_function.baml:18:13 (string) len=1 ":"
// llm_function.baml:18:17 (parameter) len=5 "email"
// llm_function.baml:18:23 (property) len=4 "body"
// llm_function.baml:20:15 (property) len=13 "output_format"
// llm_function.baml:21:5 (string) len=1 "`"
