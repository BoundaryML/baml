// Namespace: root.ansi — ANSI terminal styling helpers.
//
// BAML string literals don't have an \x1b / \e escape, so we build the ESC
// byte (codepoint 27) with string.from_code_points, then wrap text in SGR
// codes. Honors the NO_COLOR convention (https://no-color.org): if the
// NO_COLOR env var is set to anything, styling is a no-op.
//
// Used from the root namespace as root.ansi.red("..."), root.ansi.bold(...), etc.

function esc() -> string {
    string.from_code_points([27])
}

// Wrap `text` in an SGR parameter (e.g. "31" for red) and reset afterwards.
function sgr(code: string, text: string) -> string {
    match (baml.env.get("NO_COLOR")) {
        null => esc() + "[" + code + "m" + text + esc() + "[0m",
        _ => text,
    }
}

// Attributes
function bold(text: string) -> string {
    sgr("1", text)
}

function dim(text: string) -> string {
    sgr("2", text)
}

function italic(text: string) -> string {
    sgr("3", text)
}

function underline(text: string) -> string {
    sgr("4", text)
}

// Foreground colors
function red(text: string) -> string {
    sgr("31", text)
}

function green(text: string) -> string {
    sgr("32", text)
}

function yellow(text: string) -> string {
    sgr("33", text)
}

function blue(text: string) -> string {
    sgr("34", text)
}

function magenta(text: string) -> string {
    sgr("35", text)
}

function cyan(text: string) -> string {
    sgr("36", text)
}

function gray(text: string) -> string {
    sgr("90", text)
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- semantic_tokens
// ns_ansi.baml:1:1 (comment) len=58 "// Namespace: root.ansi — ANSI terminal styling helpers."
// ns_ansi.baml:2:1 (comment) len=2 "//"
// ns_ansi.baml:3:1 (comment) len=75 "// BAML string literals don't have an \\x1b / \\e escape, so we build the ESC"
// ns_ansi.baml:4:1 (comment) len=74 "// byte (codepoint 27) with string.from_code_points, then wrap text in SGR"
// ns_ansi.baml:5:1 (comment) len=71 "// codes. Honors the NO_COLOR convention (https://no-color.org): if the"
// ns_ansi.baml:6:1 (comment) len=59 "// NO_COLOR env var is set to anything, styling is a no-op."
// ns_ansi.baml:7:1 (comment) len=2 "//"
// ns_ansi.baml:8:1 (comment) len=82 "// Used from the root namespace as root.ansi.red(\"...\"), root.ansi.bold(...), etc."
// ns_ansi.baml:10:1 (keyword) len=8 "function"
// ns_ansi.baml:10:10 (function) [declaration] len=3 "esc"
// ns_ansi.baml:10:19 (type) [defaultLibrary] len=6 "string"
// ns_ansi.baml:11:5 (type) [defaultLibrary] len=6 "string"
// ns_ansi.baml:11:12 (method) len=16 "from_code_points"
// ns_ansi.baml:11:30 (number) len=2 "27"
// ns_ansi.baml:14:1 (comment) len=76 "// Wrap `text` in an SGR parameter (e.g. \"31\" for red) and reset afterwards."
// ns_ansi.baml:15:1 (keyword) len=8 "function"
// ns_ansi.baml:15:10 (function) [declaration] len=3 "sgr"
// ns_ansi.baml:15:14 (parameter) [declaration] len=4 "code"
// ns_ansi.baml:15:20 (type) [defaultLibrary] len=6 "string"
// ns_ansi.baml:15:28 (parameter) [declaration] len=4 "text"
// ns_ansi.baml:15:34 (type) [defaultLibrary] len=6 "string"
// ns_ansi.baml:15:45 (type) [defaultLibrary] len=6 "string"
// ns_ansi.baml:16:5 (keyword) len=5 "match"
// ns_ansi.baml:16:12 (namespace) [defaultLibrary] len=4 "baml"
// ns_ansi.baml:16:16 (namespace) [defaultLibrary] len=1 "."
// ns_ansi.baml:16:17 (namespace) [defaultLibrary] len=3 "env"
// ns_ansi.baml:16:20 (namespace) [defaultLibrary] len=1 "."
// ns_ansi.baml:16:21 (function) len=3 "get"
// ns_ansi.baml:16:25 (string) len=10 "\"NO_COLOR\""
// ns_ansi.baml:17:9 (type) [defaultLibrary] len=4 "null"
// ns_ansi.baml:17:17 (function) len=3 "esc"
// ns_ansi.baml:17:23 (operator) len=1 "+"
// ns_ansi.baml:17:25 (string) len=3 "\"[\""
// ns_ansi.baml:17:29 (operator) len=1 "+"
// ns_ansi.baml:17:31 (parameter) len=4 "code"
// ns_ansi.baml:17:36 (operator) len=1 "+"
// ns_ansi.baml:17:38 (string) len=3 "\"m\""
// ns_ansi.baml:17:42 (operator) len=1 "+"
// ns_ansi.baml:17:44 (parameter) len=4 "text"
// ns_ansi.baml:17:49 (operator) len=1 "+"
// ns_ansi.baml:17:51 (function) len=3 "esc"
// ns_ansi.baml:17:57 (operator) len=1 "+"
// ns_ansi.baml:17:59 (string) len=5 "\"[0m\""
// ns_ansi.baml:18:14 (parameter) len=4 "text"
// ns_ansi.baml:22:1 (comment) len=13 "// Attributes"
// ns_ansi.baml:23:1 (keyword) len=8 "function"
// ns_ansi.baml:23:10 (function) [declaration] len=4 "bold"
// ns_ansi.baml:23:15 (parameter) [declaration] len=4 "text"
// ns_ansi.baml:23:21 (type) [defaultLibrary] len=6 "string"
// ns_ansi.baml:23:32 (type) [defaultLibrary] len=6 "string"
// ns_ansi.baml:24:5 (function) len=3 "sgr"
// ns_ansi.baml:24:9 (string) len=3 "\"1\""
// ns_ansi.baml:24:14 (parameter) len=4 "text"
// ns_ansi.baml:27:1 (keyword) len=8 "function"
// ns_ansi.baml:27:10 (function) [declaration] len=3 "dim"
// ns_ansi.baml:27:14 (parameter) [declaration] len=4 "text"
// ns_ansi.baml:27:20 (type) [defaultLibrary] len=6 "string"
// ns_ansi.baml:27:31 (type) [defaultLibrary] len=6 "string"
// ns_ansi.baml:28:5 (function) len=3 "sgr"
// ns_ansi.baml:28:9 (string) len=3 "\"2\""
// ns_ansi.baml:28:14 (parameter) len=4 "text"
// ns_ansi.baml:31:1 (keyword) len=8 "function"
// ns_ansi.baml:31:10 (function) [declaration] len=6 "italic"
// ns_ansi.baml:31:17 (parameter) [declaration] len=4 "text"
// ns_ansi.baml:31:23 (type) [defaultLibrary] len=6 "string"
// ns_ansi.baml:31:34 (type) [defaultLibrary] len=6 "string"
// ns_ansi.baml:32:5 (function) len=3 "sgr"
// ns_ansi.baml:32:9 (string) len=3 "\"3\""
// ns_ansi.baml:32:14 (parameter) len=4 "text"
// ns_ansi.baml:35:1 (keyword) len=8 "function"
// ns_ansi.baml:35:10 (function) [declaration] len=9 "underline"
// ns_ansi.baml:35:20 (parameter) [declaration] len=4 "text"
// ns_ansi.baml:35:26 (type) [defaultLibrary] len=6 "string"
// ns_ansi.baml:35:37 (type) [defaultLibrary] len=6 "string"
// ns_ansi.baml:36:5 (function) len=3 "sgr"
// ns_ansi.baml:36:9 (string) len=3 "\"4\""
// ns_ansi.baml:36:14 (parameter) len=4 "text"
// ns_ansi.baml:39:1 (comment) len=20 "// Foreground colors"
// ns_ansi.baml:40:1 (keyword) len=8 "function"
// ns_ansi.baml:40:10 (function) [declaration] len=3 "red"
// ns_ansi.baml:40:14 (parameter) [declaration] len=4 "text"
// ns_ansi.baml:40:20 (type) [defaultLibrary] len=6 "string"
// ns_ansi.baml:40:31 (type) [defaultLibrary] len=6 "string"
// ns_ansi.baml:41:5 (function) len=3 "sgr"
// ns_ansi.baml:41:9 (string) len=4 "\"31\""
// ns_ansi.baml:41:15 (parameter) len=4 "text"
// ns_ansi.baml:44:1 (keyword) len=8 "function"
// ns_ansi.baml:44:10 (function) [declaration] len=5 "green"
// ns_ansi.baml:44:16 (parameter) [declaration] len=4 "text"
// ns_ansi.baml:44:22 (type) [defaultLibrary] len=6 "string"
// ns_ansi.baml:44:33 (type) [defaultLibrary] len=6 "string"
// ns_ansi.baml:45:5 (function) len=3 "sgr"
// ns_ansi.baml:45:9 (string) len=4 "\"32\""
// ns_ansi.baml:45:15 (parameter) len=4 "text"
// ns_ansi.baml:48:1 (keyword) len=8 "function"
// ns_ansi.baml:48:10 (function) [declaration] len=6 "yellow"
// ns_ansi.baml:48:17 (parameter) [declaration] len=4 "text"
// ns_ansi.baml:48:23 (type) [defaultLibrary] len=6 "string"
// ns_ansi.baml:48:34 (type) [defaultLibrary] len=6 "string"
// ns_ansi.baml:49:5 (function) len=3 "sgr"
// ns_ansi.baml:49:9 (string) len=4 "\"33\""
// ns_ansi.baml:49:15 (parameter) len=4 "text"
// ns_ansi.baml:52:1 (keyword) len=8 "function"
// ns_ansi.baml:52:10 (function) [declaration] len=4 "blue"
// ns_ansi.baml:52:15 (parameter) [declaration] len=4 "text"
// ns_ansi.baml:52:21 (type) [defaultLibrary] len=6 "string"
// ns_ansi.baml:52:32 (type) [defaultLibrary] len=6 "string"
// ns_ansi.baml:53:5 (function) len=3 "sgr"
// ns_ansi.baml:53:9 (string) len=4 "\"34\""
// ns_ansi.baml:53:15 (parameter) len=4 "text"
// ns_ansi.baml:56:1 (keyword) len=8 "function"
// ns_ansi.baml:56:10 (function) [declaration] len=7 "magenta"
// ns_ansi.baml:56:18 (parameter) [declaration] len=4 "text"
// ns_ansi.baml:56:24 (type) [defaultLibrary] len=6 "string"
// ns_ansi.baml:56:35 (type) [defaultLibrary] len=6 "string"
// ns_ansi.baml:57:5 (function) len=3 "sgr"
// ns_ansi.baml:57:9 (string) len=4 "\"35\""
// ns_ansi.baml:57:15 (parameter) len=4 "text"
// ns_ansi.baml:60:1 (keyword) len=8 "function"
// ns_ansi.baml:60:10 (function) [declaration] len=4 "cyan"
// ns_ansi.baml:60:15 (parameter) [declaration] len=4 "text"
// ns_ansi.baml:60:21 (type) [defaultLibrary] len=6 "string"
// ns_ansi.baml:60:32 (type) [defaultLibrary] len=6 "string"
// ns_ansi.baml:61:5 (function) len=3 "sgr"
// ns_ansi.baml:61:9 (string) len=4 "\"36\""
// ns_ansi.baml:61:15 (parameter) len=4 "text"
// ns_ansi.baml:64:1 (keyword) len=8 "function"
// ns_ansi.baml:64:10 (function) [declaration] len=4 "gray"
// ns_ansi.baml:64:15 (parameter) [declaration] len=4 "text"
// ns_ansi.baml:64:21 (type) [defaultLibrary] len=6 "string"
// ns_ansi.baml:64:32 (type) [defaultLibrary] len=6 "string"
// ns_ansi.baml:65:5 (function) len=3 "sgr"
// ns_ansi.baml:65:9 (string) len=4 "\"90\""
// ns_ansi.baml:65:15 (parameter) len=4 "text"
