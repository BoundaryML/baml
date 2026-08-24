type ProjectId = int

function NormalAlias(pid: ProjectId) -> string {
  client: "openai/gpt-4o"
  prompt: `Pid: ${pid}. Generate a fake name for it.`
}

type A = float
type B = A
type C = B

function LongerAlias(c: C) -> string {
  client: "openai/gpt-4o"
  prompt: `${c}`
}

type JsonValue = int | string | bool | float | JsonObject | JsonArray
type JsonObject = map<string, JsonValue>
type JsonArray = JsonValue[]

function RecursiveAliases(j: JsonValue) -> string {
  client: "openai/gpt-4o"
  prompt: `${j}`
}

type I = J
type J = I

function InvalidAlias(i: I) -> string {
  client: "openai/gpt-4o"
  prompt: `${i}`
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- semantic_tokens
// type_aliases_jinja.baml:1:1 (keyword) len=4 "type"
// type_aliases_jinja.baml:1:6 (type) [declaration] len=9 "ProjectId"
// type_aliases_jinja.baml:1:16 (operator) len=1 "="
// type_aliases_jinja.baml:1:18 (type) [defaultLibrary] len=3 "int"
// type_aliases_jinja.baml:3:1 (keyword) len=8 "function"
// type_aliases_jinja.baml:3:10 (function) [declaration] len=11 "NormalAlias"
// type_aliases_jinja.baml:3:22 (parameter) [declaration] len=3 "pid"
// type_aliases_jinja.baml:3:27 (type) len=9 "ProjectId"
// type_aliases_jinja.baml:3:41 (type) [defaultLibrary] len=6 "string"
// type_aliases_jinja.baml:4:3 (property) len=6 "client"
// type_aliases_jinja.baml:4:11 (string) len=15 "\"openai/gpt-4o\""
// type_aliases_jinja.baml:5:3 (property) [declaration] len=6 "prompt"
// type_aliases_jinja.baml:5:11 (string) len=1 "`"
// type_aliases_jinja.baml:5:12 (string) len=3 "Pid"
// type_aliases_jinja.baml:5:15 (string) len=1 ":"
// type_aliases_jinja.baml:5:19 (parameter) len=3 "pid"
// type_aliases_jinja.baml:5:23 (string) len=1 "."
// type_aliases_jinja.baml:5:25 (string) len=8 "Generate"
// type_aliases_jinja.baml:5:34 (string) len=1 "a"
// type_aliases_jinja.baml:5:36 (string) len=4 "fake"
// type_aliases_jinja.baml:5:41 (string) len=4 "name"
// type_aliases_jinja.baml:5:46 (string) len=3 "for"
// type_aliases_jinja.baml:5:50 (string) len=2 "it"
// type_aliases_jinja.baml:5:52 (string) len=1 "."
// type_aliases_jinja.baml:5:53 (string) len=1 "`"
// type_aliases_jinja.baml:8:1 (keyword) len=4 "type"
// type_aliases_jinja.baml:8:6 (type) [declaration] len=1 "A"
// type_aliases_jinja.baml:8:8 (operator) len=1 "="
// type_aliases_jinja.baml:8:10 (type) [defaultLibrary] len=5 "float"
// type_aliases_jinja.baml:9:1 (keyword) len=4 "type"
// type_aliases_jinja.baml:9:6 (type) [declaration] len=1 "B"
// type_aliases_jinja.baml:9:8 (operator) len=1 "="
// type_aliases_jinja.baml:9:10 (type) len=1 "A"
// type_aliases_jinja.baml:10:1 (keyword) len=4 "type"
// type_aliases_jinja.baml:10:6 (type) [declaration] len=1 "C"
// type_aliases_jinja.baml:10:8 (operator) len=1 "="
// type_aliases_jinja.baml:10:10 (type) len=1 "B"
// type_aliases_jinja.baml:12:1 (keyword) len=8 "function"
// type_aliases_jinja.baml:12:10 (function) [declaration] len=11 "LongerAlias"
// type_aliases_jinja.baml:12:22 (parameter) [declaration] len=1 "c"
// type_aliases_jinja.baml:12:25 (type) len=1 "C"
// type_aliases_jinja.baml:12:31 (type) [defaultLibrary] len=6 "string"
// type_aliases_jinja.baml:13:3 (property) len=6 "client"
// type_aliases_jinja.baml:13:11 (string) len=15 "\"openai/gpt-4o\""
// type_aliases_jinja.baml:14:3 (property) [declaration] len=6 "prompt"
// type_aliases_jinja.baml:14:11 (string) len=1 "`"
// type_aliases_jinja.baml:14:14 (parameter) len=1 "c"
// type_aliases_jinja.baml:14:16 (string) len=1 "`"
// type_aliases_jinja.baml:17:1 (keyword) len=4 "type"
// type_aliases_jinja.baml:17:6 (type) [declaration] len=9 "JsonValue"
// type_aliases_jinja.baml:17:16 (operator) len=1 "="
// type_aliases_jinja.baml:17:18 (type) [defaultLibrary] len=3 "int"
// type_aliases_jinja.baml:17:22 (operator) len=1 "|"
// type_aliases_jinja.baml:17:24 (type) [defaultLibrary] len=6 "string"
// type_aliases_jinja.baml:17:31 (operator) len=1 "|"
// type_aliases_jinja.baml:17:33 (type) [defaultLibrary] len=4 "bool"
// type_aliases_jinja.baml:17:38 (operator) len=1 "|"
// type_aliases_jinja.baml:17:40 (type) [defaultLibrary] len=5 "float"
// type_aliases_jinja.baml:17:46 (operator) len=1 "|"
// type_aliases_jinja.baml:17:48 (type) len=10 "JsonObject"
// type_aliases_jinja.baml:17:59 (operator) len=1 "|"
// type_aliases_jinja.baml:17:61 (type) len=9 "JsonArray"
// type_aliases_jinja.baml:18:1 (keyword) len=4 "type"
// type_aliases_jinja.baml:18:6 (type) [declaration] len=10 "JsonObject"
// type_aliases_jinja.baml:18:17 (operator) len=1 "="
// type_aliases_jinja.baml:18:19 (type) [defaultLibrary] len=3 "map"
// type_aliases_jinja.baml:18:22 (operator) len=1 "<"
// type_aliases_jinja.baml:18:23 (type) [defaultLibrary] len=6 "string"
// type_aliases_jinja.baml:18:31 (type) len=9 "JsonValue"
// type_aliases_jinja.baml:18:40 (operator) len=1 ">"
// type_aliases_jinja.baml:19:1 (keyword) len=4 "type"
// type_aliases_jinja.baml:19:6 (type) [declaration] len=9 "JsonArray"
// type_aliases_jinja.baml:19:16 (operator) len=1 "="
// type_aliases_jinja.baml:19:18 (type) len=9 "JsonValue"
// type_aliases_jinja.baml:21:1 (keyword) len=8 "function"
// type_aliases_jinja.baml:21:10 (function) [declaration] len=16 "RecursiveAliases"
// type_aliases_jinja.baml:21:27 (parameter) [declaration] len=1 "j"
// type_aliases_jinja.baml:21:30 (type) len=9 "JsonValue"
// type_aliases_jinja.baml:21:44 (type) [defaultLibrary] len=6 "string"
// type_aliases_jinja.baml:22:3 (property) len=6 "client"
// type_aliases_jinja.baml:22:11 (string) len=15 "\"openai/gpt-4o\""
// type_aliases_jinja.baml:23:3 (property) [declaration] len=6 "prompt"
// type_aliases_jinja.baml:23:11 (string) len=1 "`"
// type_aliases_jinja.baml:23:14 (parameter) len=1 "j"
// type_aliases_jinja.baml:23:16 (string) len=1 "`"
// type_aliases_jinja.baml:26:1 (keyword) len=4 "type"
// type_aliases_jinja.baml:26:6 (type) [declaration] len=1 "I"
// type_aliases_jinja.baml:26:8 (operator) len=1 "="
// type_aliases_jinja.baml:26:10 (type) len=1 "J"
// type_aliases_jinja.baml:27:1 (keyword) len=4 "type"
// type_aliases_jinja.baml:27:6 (type) [declaration] len=1 "J"
// type_aliases_jinja.baml:27:8 (operator) len=1 "="
// type_aliases_jinja.baml:27:10 (type) len=1 "I"
// type_aliases_jinja.baml:29:1 (keyword) len=8 "function"
// type_aliases_jinja.baml:29:10 (function) [declaration] len=12 "InvalidAlias"
// type_aliases_jinja.baml:29:23 (parameter) [declaration] len=1 "i"
// type_aliases_jinja.baml:29:26 (type) len=1 "I"
// type_aliases_jinja.baml:29:32 (type) [defaultLibrary] len=6 "string"
// type_aliases_jinja.baml:30:3 (property) len=6 "client"
// type_aliases_jinja.baml:30:11 (string) len=15 "\"openai/gpt-4o\""
// type_aliases_jinja.baml:31:3 (property) [declaration] len=6 "prompt"
// type_aliases_jinja.baml:31:11 (string) len=1 "`"
// type_aliases_jinja.baml:31:14 (parameter) len=1 "i"
// type_aliases_jinja.baml:31:16 (string) len=1 "`"
