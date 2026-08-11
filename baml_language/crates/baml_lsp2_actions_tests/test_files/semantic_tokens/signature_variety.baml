function InputNone() -> int {
    client: "openai/gpt-4o"
    prompt: `fa`
}

function InputSimple(a: string, b: int | bool) -> int {
    client: "openai/gpt-4o"
    prompt: `fa`
}

class Email {
    subject: string,
    body: string,
}

function InputObject(email: Email) -> string {
    client: "openai/gpt-4o"
    prompt: ` subject line ${email.subject} `
}

enum Color {
    RED,
    BLUE,
    GREEN,
}

function InputEnum(color: Color) -> string {
    client: "openai/gpt-4o"
    prompt: ` color ${color} `
}

// Try it with wierd spacing and comments

function InputEnum2(
    color: Color,
    bar: string, // ok
    // This param is great!
    foo: string[],
    // But we should do something...
    baz: (string | int?)[],
) -> // Do something
    string {
    client: "openai/gpt-4o"
    prompt: ` color ${color} `
}

function InputImage(img: image) -> string {
    client: "openai/gpt-4o"
    prompt: ` image ${img} `
}

// Shorthand client strings must be quoted
function ExtractName(text: string) -> string {
    client: "openai/gpt-4o-mini"
    prompt: `
        Extract the person's name from this text: ${text}
        Return only the name, nothing else.
    `
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- semantic_tokens
// signature_variety.baml:1:1 (keyword) len=8 "function"
// signature_variety.baml:1:10 (function) [declaration] len=9 "InputNone"
// signature_variety.baml:1:25 (type) [defaultLibrary] len=3 "int"
// signature_variety.baml:2:5 (property) len=6 "client"
// signature_variety.baml:2:13 (string) len=15 "\"openai/gpt-4o\""
// signature_variety.baml:3:5 (property) [declaration] len=6 "prompt"
// signature_variety.baml:3:13 (string) len=1 "`"
// signature_variety.baml:3:14 (string) len=2 "fa"
// signature_variety.baml:3:16 (string) len=1 "`"
// signature_variety.baml:6:1 (keyword) len=8 "function"
// signature_variety.baml:6:10 (function) [declaration] len=11 "InputSimple"
// signature_variety.baml:6:22 (parameter) [declaration] len=1 "a"
// signature_variety.baml:6:25 (type) [defaultLibrary] len=6 "string"
// signature_variety.baml:6:33 (parameter) [declaration] len=1 "b"
// signature_variety.baml:6:36 (type) [defaultLibrary] len=3 "int"
// signature_variety.baml:6:40 (operator) len=1 "|"
// signature_variety.baml:6:42 (type) [defaultLibrary] len=4 "bool"
// signature_variety.baml:6:51 (type) [defaultLibrary] len=3 "int"
// signature_variety.baml:7:5 (property) len=6 "client"
// signature_variety.baml:7:13 (string) len=15 "\"openai/gpt-4o\""
// signature_variety.baml:8:5 (property) [declaration] len=6 "prompt"
// signature_variety.baml:8:13 (string) len=1 "`"
// signature_variety.baml:8:14 (string) len=2 "fa"
// signature_variety.baml:8:16 (string) len=1 "`"
// signature_variety.baml:11:1 (keyword) len=5 "class"
// signature_variety.baml:11:7 (class) [declaration] len=5 "Email"
// signature_variety.baml:12:5 (property) [declaration] len=7 "subject"
// signature_variety.baml:12:14 (type) [defaultLibrary] len=6 "string"
// signature_variety.baml:13:5 (property) [declaration] len=4 "body"
// signature_variety.baml:13:11 (type) [defaultLibrary] len=6 "string"
// signature_variety.baml:16:1 (keyword) len=8 "function"
// signature_variety.baml:16:10 (function) [declaration] len=11 "InputObject"
// signature_variety.baml:16:22 (parameter) [declaration] len=5 "email"
// signature_variety.baml:16:29 (class) len=5 "Email"
// signature_variety.baml:16:39 (type) [defaultLibrary] len=6 "string"
// signature_variety.baml:17:5 (property) len=6 "client"
// signature_variety.baml:17:13 (string) len=15 "\"openai/gpt-4o\""
// signature_variety.baml:18:5 (property) [declaration] len=6 "prompt"
// signature_variety.baml:18:13 (string) len=1 "`"
// signature_variety.baml:18:15 (string) len=7 "subject"
// signature_variety.baml:18:23 (string) len=4 "line"
// signature_variety.baml:18:30 (parameter) len=5 "email"
// signature_variety.baml:18:36 (property) len=7 "subject"
// signature_variety.baml:18:45 (string) len=1 "`"
// signature_variety.baml:21:1 (keyword) len=4 "enum"
// signature_variety.baml:21:6 (enum) [declaration] len=5 "Color"
// signature_variety.baml:22:5 (enumMember) [declaration] len=3 "RED"
// signature_variety.baml:23:5 (enumMember) [declaration] len=4 "BLUE"
// signature_variety.baml:24:5 (enumMember) [declaration] len=5 "GREEN"
// signature_variety.baml:27:1 (keyword) len=8 "function"
// signature_variety.baml:27:10 (function) [declaration] len=9 "InputEnum"
// signature_variety.baml:27:20 (parameter) [declaration] len=5 "color"
// signature_variety.baml:27:27 (enum) len=5 "Color"
// signature_variety.baml:27:37 (type) [defaultLibrary] len=6 "string"
// signature_variety.baml:28:5 (property) len=6 "client"
// signature_variety.baml:28:13 (string) len=15 "\"openai/gpt-4o\""
// signature_variety.baml:29:5 (property) [declaration] len=6 "prompt"
// signature_variety.baml:29:13 (string) len=1 "`"
// signature_variety.baml:29:15 (string) len=5 "color"
// signature_variety.baml:29:23 (parameter) len=5 "color"
// signature_variety.baml:29:30 (string) len=1 "`"
// signature_variety.baml:32:1 (comment) len=41 "// Try it with wierd spacing and comments"
// signature_variety.baml:34:1 (keyword) len=8 "function"
// signature_variety.baml:34:10 (function) [declaration] len=10 "InputEnum2"
// signature_variety.baml:35:5 (parameter) [declaration] len=5 "color"
// signature_variety.baml:35:12 (enum) len=5 "Color"
// signature_variety.baml:36:5 (parameter) [declaration] len=3 "bar"
// signature_variety.baml:36:10 (type) [defaultLibrary] len=6 "string"
// signature_variety.baml:36:18 (comment) len=5 "// ok"
// signature_variety.baml:37:5 (comment) len=23 "// This param is great!"
// signature_variety.baml:38:5 (parameter) [declaration] len=3 "foo"
// signature_variety.baml:38:10 (type) [defaultLibrary] len=6 "string"
// signature_variety.baml:39:5 (comment) len=32 "// But we should do something..."
// signature_variety.baml:40:5 (parameter) [declaration] len=3 "baz"
// signature_variety.baml:40:11 (type) [defaultLibrary] len=6 "string"
// signature_variety.baml:40:18 (operator) len=1 "|"
// signature_variety.baml:40:20 (type) [defaultLibrary] len=3 "int"
// signature_variety.baml:41:6 (comment) len=15 "// Do something"
// signature_variety.baml:42:5 (type) [defaultLibrary] len=6 "string"
// signature_variety.baml:43:5 (property) len=6 "client"
// signature_variety.baml:43:13 (string) len=15 "\"openai/gpt-4o\""
// signature_variety.baml:44:5 (property) [declaration] len=6 "prompt"
// signature_variety.baml:44:13 (string) len=1 "`"
// signature_variety.baml:44:15 (string) len=5 "color"
// signature_variety.baml:44:23 (parameter) len=5 "color"
// signature_variety.baml:44:30 (string) len=1 "`"
// signature_variety.baml:47:1 (keyword) len=8 "function"
// signature_variety.baml:47:10 (function) [declaration] len=10 "InputImage"
// signature_variety.baml:47:21 (parameter) [declaration] len=3 "img"
// signature_variety.baml:47:26 (type) [defaultLibrary] len=5 "image"
// signature_variety.baml:47:36 (type) [defaultLibrary] len=6 "string"
// signature_variety.baml:48:5 (property) len=6 "client"
// signature_variety.baml:48:13 (string) len=15 "\"openai/gpt-4o\""
// signature_variety.baml:49:5 (property) [declaration] len=6 "prompt"
// signature_variety.baml:49:13 (string) len=1 "`"
// signature_variety.baml:49:15 (string) len=5 "image"
// signature_variety.baml:49:23 (parameter) len=3 "img"
// signature_variety.baml:49:28 (string) len=1 "`"
// signature_variety.baml:52:1 (comment) len=42 "// Shorthand client strings must be quoted"
// signature_variety.baml:53:1 (keyword) len=8 "function"
// signature_variety.baml:53:10 (function) [declaration] len=11 "ExtractName"
// signature_variety.baml:53:22 (parameter) [declaration] len=4 "text"
// signature_variety.baml:53:28 (type) [defaultLibrary] len=6 "string"
// signature_variety.baml:53:39 (type) [defaultLibrary] len=6 "string"
// signature_variety.baml:54:5 (property) len=6 "client"
// signature_variety.baml:54:13 (string) len=20 "\"openai/gpt-4o-mini\""
// signature_variety.baml:55:5 (property) [declaration] len=6 "prompt"
// signature_variety.baml:55:13 (string) len=1 "`"
// signature_variety.baml:56:9 (string) len=7 "Extract"
// signature_variety.baml:56:17 (string) len=3 "the"
// signature_variety.baml:56:21 (string) len=6 "person"
// signature_variety.baml:56:27 (string) len=1 "'"
// signature_variety.baml:56:28 (string) len=1 "s"
// signature_variety.baml:56:30 (string) len=4 "name"
// signature_variety.baml:56:35 (string) len=4 "from"
// signature_variety.baml:56:40 (string) len=4 "this"
// signature_variety.baml:56:45 (string) len=4 "text"
// signature_variety.baml:56:49 (string) len=1 ":"
// signature_variety.baml:56:53 (parameter) len=4 "text"
// signature_variety.baml:57:9 (string) len=6 "Return"
// signature_variety.baml:57:16 (string) len=4 "only"
// signature_variety.baml:57:21 (string) len=3 "the"
// signature_variety.baml:57:25 (string) len=4 "name"
// signature_variety.baml:57:29 (string) len=1 ","
// signature_variety.baml:57:31 (string) len=7 "nothing"
// signature_variety.baml:57:39 (string) len=4 "else"
// signature_variety.baml:57:43 (string) len=1 "."
// signature_variety.baml:58:5 (string) len=1 "`"
