type ProjectId = int

function NormalAlias(pid: ProjectId) -> string {
  client "openai/gpt-4o"
  prompt #"Pid: {{ pid.id }}. Generate a fake name for it."#
}

type A = float
type B = A
type C = B

function LongerAlias(c: C) -> string {
  client "openai/gpt-4o"
  prompt #"{{ c.value }}"#
}

type JsonValue = int | string | bool | float | JsonObject | JsonArray
type JsonObject = map<string, JsonValue>
type JsonArray = JsonValue[]

function RecursiveAliases(j: JsonValue) -> string {
  client "openai/gpt-4o"
  prompt #"{{ j.value }}"#
}

type I = J
type J = I

function InvalidAlias(i: I) -> string {
  client "openai/gpt-4o"
  prompt #"{{ i.value }}"#
}

// warning: 'pid' is a type alias ProjectId (resolves to int), expected class
//   -->  class/type_aliases_jinja.baml:5
//    |
//  4 |   client "openai/gpt-4o"
//  5 |   prompt #"Pid: {{ pid.id }}. Generate a fake name for it."#
//    |
// warning: 'c' is a type alias C (resolves to float), expected class
//   -->  class/type_aliases_jinja.baml:14
//    |
// 13 |   client "openai/gpt-4o"
// 14 |   prompt #"{{ c.value }}"#
//    |
// warning: 'j' is a recursive type alias JsonValue, expected class
//   -->  class/type_aliases_jinja.baml:23
//    |
// 22 |   client "openai/gpt-4o"
// 23 |   prompt #"{{ j.value }}"#
//    |
// warning: 'i' is a recursive type alias I, expected class
//   -->  class/type_aliases_jinja.baml:31
//    |
// 30 |   client "openai/gpt-4o"
// 31 |   prompt #"{{ i.value }}"#
//    |
// error: Error validating: These aliases form a dependency cycle: I -> J
//   -->  class/type_aliases_jinja.baml:26
//    |
// 25 |
// 26 | type I = J
//    |

//----
//- diagnostics
// E0073
//
//   ⚠ 'pid' is a type alias ProjectId (resolves to int), expected class
//    ╭─[type_aliases_jinja.baml:5:20]
//  5 │   prompt #"Pid: {{ pid.id }}. Generate a fake name for it."#
//    ·                    ──────
//    ╰────
// E0073
//
//   ⚠ 'c' is a type alias C (resolves to type alias B (resolves to type alias A (resolves to float))), expected class
//     ╭─[type_aliases_jinja.baml:14:15]
//  14 │   prompt #"{{ c.value }}"#
//     ·               ───────
//     ╰────
// E0074
//
//   ⚠ cannot access property 'value' on 'j': union contains non-class type map[string, recursive type alias JsonValue]
//     ╭─[type_aliases_jinja.baml:23:15]
//  23 │   prompt #"{{ j.value }}"#
//     ·               ───────
//     ╰────
// E0073
//
//   ⚠ 'i' is a type alias I (resolves to type alias J (resolves to recursive type alias I)), expected class
//     ╭─[type_aliases_jinja.baml:31:15]
//  31 │   prompt #"{{ i.value }}"#
//     ·               ───────
//     ╰────
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
// type_aliases_jinja.baml:4:10 (string) len=15 "\"openai/gpt-4o\""
// type_aliases_jinja.baml:5:3 (property) [declaration] len=6 "prompt"
// type_aliases_jinja.baml:5:10 (string) len=1 "#"
// type_aliases_jinja.baml:5:11 (string) len=1 "\""
// type_aliases_jinja.baml:5:12 (string) len=3 "Pid"
// type_aliases_jinja.baml:5:15 (string) len=1 ":"
// type_aliases_jinja.baml:5:17 (string) len=1 "{"
// type_aliases_jinja.baml:5:18 (string) len=1 "{"
// type_aliases_jinja.baml:5:20 (string) len=3 "pid"
// type_aliases_jinja.baml:5:23 (string) len=1 "."
// type_aliases_jinja.baml:5:24 (string) len=2 "id"
// type_aliases_jinja.baml:5:27 (string) len=1 "}"
// type_aliases_jinja.baml:5:28 (string) len=1 "}"
// type_aliases_jinja.baml:5:29 (string) len=1 "."
// type_aliases_jinja.baml:5:31 (string) len=8 "Generate"
// type_aliases_jinja.baml:5:40 (string) len=1 "a"
// type_aliases_jinja.baml:5:42 (string) len=4 "fake"
// type_aliases_jinja.baml:5:47 (string) len=4 "name"
// type_aliases_jinja.baml:5:52 (string) len=3 "for"
// type_aliases_jinja.baml:5:56 (string) len=2 "it"
// type_aliases_jinja.baml:5:58 (string) len=1 "."
// type_aliases_jinja.baml:5:59 (string) len=1 "\""
// type_aliases_jinja.baml:5:60 (string) len=1 "#"
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
// type_aliases_jinja.baml:13:10 (string) len=15 "\"openai/gpt-4o\""
// type_aliases_jinja.baml:14:3 (property) [declaration] len=6 "prompt"
// type_aliases_jinja.baml:14:10 (string) len=1 "#"
// type_aliases_jinja.baml:14:11 (string) len=1 "\""
// type_aliases_jinja.baml:14:12 (string) len=1 "{"
// type_aliases_jinja.baml:14:13 (string) len=1 "{"
// type_aliases_jinja.baml:14:15 (string) len=1 "c"
// type_aliases_jinja.baml:14:16 (string) len=1 "."
// type_aliases_jinja.baml:14:17 (string) len=5 "value"
// type_aliases_jinja.baml:14:23 (string) len=1 "}"
// type_aliases_jinja.baml:14:24 (string) len=1 "}"
// type_aliases_jinja.baml:14:25 (string) len=1 "\""
// type_aliases_jinja.baml:14:26 (string) len=1 "#"
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
// type_aliases_jinja.baml:22:10 (string) len=15 "\"openai/gpt-4o\""
// type_aliases_jinja.baml:23:3 (property) [declaration] len=6 "prompt"
// type_aliases_jinja.baml:23:10 (string) len=1 "#"
// type_aliases_jinja.baml:23:11 (string) len=1 "\""
// type_aliases_jinja.baml:23:12 (string) len=1 "{"
// type_aliases_jinja.baml:23:13 (string) len=1 "{"
// type_aliases_jinja.baml:23:15 (string) len=1 "j"
// type_aliases_jinja.baml:23:16 (string) len=1 "."
// type_aliases_jinja.baml:23:17 (string) len=5 "value"
// type_aliases_jinja.baml:23:23 (string) len=1 "}"
// type_aliases_jinja.baml:23:24 (string) len=1 "}"
// type_aliases_jinja.baml:23:25 (string) len=1 "\""
// type_aliases_jinja.baml:23:26 (string) len=1 "#"
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
// type_aliases_jinja.baml:30:10 (string) len=15 "\"openai/gpt-4o\""
// type_aliases_jinja.baml:31:3 (property) [declaration] len=6 "prompt"
// type_aliases_jinja.baml:31:10 (string) len=1 "#"
// type_aliases_jinja.baml:31:11 (string) len=1 "\""
// type_aliases_jinja.baml:31:12 (string) len=1 "{"
// type_aliases_jinja.baml:31:13 (string) len=1 "{"
// type_aliases_jinja.baml:31:15 (string) len=1 "i"
// type_aliases_jinja.baml:31:16 (string) len=1 "."
// type_aliases_jinja.baml:31:17 (string) len=5 "value"
// type_aliases_jinja.baml:31:23 (string) len=1 "}"
// type_aliases_jinja.baml:31:24 (string) len=1 "}"
// type_aliases_jinja.baml:31:25 (string) len=1 "\""
// type_aliases_jinja.baml:31:26 (string) len=1 "#"
// type_aliases_jinja.baml:34:1 (comment) len=77 "// warning: 'pid' is a type alias ProjectId (resolves to int), expected class"
// type_aliases_jinja.baml:35:1 (comment) len=41 "//   -->  class/type_aliases_jinja.baml:5"
// type_aliases_jinja.baml:36:1 (comment) len=7 "//    |"
// type_aliases_jinja.baml:37:1 (comment) len=32 "//  4 |   client \"openai/gpt-4o\""
// type_aliases_jinja.baml:38:1 (comment) len=68 "//  5 |   prompt #\"Pid: {{ pid.id }}. Generate a fake name for it.\"#"
// type_aliases_jinja.baml:39:1 (comment) len=7 "//    |"
// type_aliases_jinja.baml:40:1 (comment) len=69 "// warning: 'c' is a type alias C (resolves to float), expected class"
// type_aliases_jinja.baml:41:1 (comment) len=42 "//   -->  class/type_aliases_jinja.baml:14"
// type_aliases_jinja.baml:42:1 (comment) len=7 "//    |"
// type_aliases_jinja.baml:43:1 (comment) len=32 "// 13 |   client \"openai/gpt-4o\""
// type_aliases_jinja.baml:44:1 (comment) len=34 "// 14 |   prompt #\"{{ c.value }}\"#"
// type_aliases_jinja.baml:45:1 (comment) len=7 "//    |"
// type_aliases_jinja.baml:46:1 (comment) len=67 "// warning: 'j' is a recursive type alias JsonValue, expected class"
// type_aliases_jinja.baml:47:1 (comment) len=42 "//   -->  class/type_aliases_jinja.baml:23"
// type_aliases_jinja.baml:48:1 (comment) len=7 "//    |"
// type_aliases_jinja.baml:49:1 (comment) len=32 "// 22 |   client \"openai/gpt-4o\""
// type_aliases_jinja.baml:50:1 (comment) len=34 "// 23 |   prompt #\"{{ j.value }}\"#"
// type_aliases_jinja.baml:51:1 (comment) len=7 "//    |"
// type_aliases_jinja.baml:52:1 (comment) len=59 "// warning: 'i' is a recursive type alias I, expected class"
// type_aliases_jinja.baml:53:1 (comment) len=42 "//   -->  class/type_aliases_jinja.baml:31"
// type_aliases_jinja.baml:54:1 (comment) len=7 "//    |"
// type_aliases_jinja.baml:55:1 (comment) len=32 "// 30 |   client \"openai/gpt-4o\""
// type_aliases_jinja.baml:56:1 (comment) len=34 "// 31 |   prompt #\"{{ i.value }}\"#"
// type_aliases_jinja.baml:57:1 (comment) len=7 "//    |"
// type_aliases_jinja.baml:58:1 (comment) len=73 "// error: Error validating: These aliases form a dependency cycle: I -> J"
// type_aliases_jinja.baml:59:1 (comment) len=42 "//   -->  class/type_aliases_jinja.baml:26"
// type_aliases_jinja.baml:60:1 (comment) len=7 "//    |"
// type_aliases_jinja.baml:61:1 (comment) len=7 "// 25 |"
// type_aliases_jinja.baml:62:1 (comment) len=18 "// 26 | type I = J"
// type_aliases_jinja.baml:63:1 (comment) len=7 "//    |"
