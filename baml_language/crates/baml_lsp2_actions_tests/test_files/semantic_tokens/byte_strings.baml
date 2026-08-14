// Byte string literal tests

function ByteStringBasic() -> int {
    let data = b"hello";
    data.length()
}

function ByteStringEmpty() -> int {
    let data = b"";
    data.length()
}

function ByteStringHexEscapes() -> int {
    let data = b"\x48\x65\x6c\x6c\x6f";
    data.length()
}

function ByteStringStandardEscapes() -> int {
    let data = b"\n\t\r\0\\\"";
    data.length()
}

function ByteStringIndexAccess() -> int? {
    let data = b"\x41\x42\x43";
    data.at(0)
}

// Byte strings in expressions
function ByteStringInCondition() -> int {
    if (b"abc".length() > 0) {
        1
    } else {
        0
    }
}

function ByteStringEquality() -> bool {
    b"abc" == b"abc"
}

// Byte string as function argument
function ByteStringPassAndReturn(data: uint8array) -> int {
    data.length()
}

// Byte string index write (mutable)
function ByteStringMutate() -> int? {
    let data = b"\x00\x00\x00";
    data[0] = 42;
    data.at(0)
}

// Direct index read (data[0], not .at(0))
function ByteStringDirectIndexRead() -> int {
    let data = b"\x41\x42\x43";
    data[0]
}

// Byte string literal as function call argument
function ByteStringLiteralAsArg() -> int {
    ByteStringPassAndReturn(b"abc")
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- semantic_tokens
// byte_strings.baml:1:1 (comment) len=28 "// Byte string literal tests"
// byte_strings.baml:3:1 (keyword) len=8 "function"
// byte_strings.baml:3:10 (function) [declaration] len=15 "ByteStringBasic"
// byte_strings.baml:3:31 (type) [defaultLibrary] len=3 "int"
// byte_strings.baml:4:5 (keyword) len=3 "let"
// byte_strings.baml:4:9 (variable) [declaration] len=4 "data"
// byte_strings.baml:4:14 (operator) len=1 "="
// byte_strings.baml:4:16 (string) len=8 "b\"hello\""
// byte_strings.baml:5:5 (variable) len=4 "data"
// byte_strings.baml:5:10 (method) len=6 "length"
// byte_strings.baml:8:1 (keyword) len=8 "function"
// byte_strings.baml:8:10 (function) [declaration] len=15 "ByteStringEmpty"
// byte_strings.baml:8:31 (type) [defaultLibrary] len=3 "int"
// byte_strings.baml:9:5 (keyword) len=3 "let"
// byte_strings.baml:9:9 (variable) [declaration] len=4 "data"
// byte_strings.baml:9:14 (operator) len=1 "="
// byte_strings.baml:9:16 (string) len=3 "b\"\""
// byte_strings.baml:10:5 (variable) len=4 "data"
// byte_strings.baml:10:10 (method) len=6 "length"
// byte_strings.baml:13:1 (keyword) len=8 "function"
// byte_strings.baml:13:10 (function) [declaration] len=20 "ByteStringHexEscapes"
// byte_strings.baml:13:36 (type) [defaultLibrary] len=3 "int"
// byte_strings.baml:14:5 (keyword) len=3 "let"
// byte_strings.baml:14:9 (variable) [declaration] len=4 "data"
// byte_strings.baml:14:14 (operator) len=1 "="
// byte_strings.baml:14:16 (string) len=2 "b\""
// byte_strings.baml:14:18 (escapeSequence) len=4 "\\x48"
// byte_strings.baml:14:22 (escapeSequence) len=4 "\\x65"
// byte_strings.baml:14:26 (escapeSequence) len=4 "\\x6c"
// byte_strings.baml:14:30 (escapeSequence) len=4 "\\x6c"
// byte_strings.baml:14:34 (escapeSequence) len=4 "\\x6f"
// byte_strings.baml:14:38 (string) len=1 "\""
// byte_strings.baml:15:5 (variable) len=4 "data"
// byte_strings.baml:15:10 (method) len=6 "length"
// byte_strings.baml:18:1 (keyword) len=8 "function"
// byte_strings.baml:18:10 (function) [declaration] len=25 "ByteStringStandardEscapes"
// byte_strings.baml:18:41 (type) [defaultLibrary] len=3 "int"
// byte_strings.baml:19:5 (keyword) len=3 "let"
// byte_strings.baml:19:9 (variable) [declaration] len=4 "data"
// byte_strings.baml:19:14 (operator) len=1 "="
// byte_strings.baml:19:16 (string) len=2 "b\""
// byte_strings.baml:19:18 (escapeSequence) len=2 "\\n"
// byte_strings.baml:19:20 (escapeSequence) len=2 "\\t"
// byte_strings.baml:19:22 (escapeSequence) len=2 "\\r"
// byte_strings.baml:19:24 (escapeSequence) len=2 "\\0"
// byte_strings.baml:19:26 (escapeSequence) len=2 "\\\\"
// byte_strings.baml:19:28 (escapeSequence) len=2 "\\\""
// byte_strings.baml:19:30 (string) len=1 "\""
// byte_strings.baml:20:5 (variable) len=4 "data"
// byte_strings.baml:20:10 (method) len=6 "length"
// byte_strings.baml:23:1 (keyword) len=8 "function"
// byte_strings.baml:23:10 (function) [declaration] len=21 "ByteStringIndexAccess"
// byte_strings.baml:23:37 (type) [defaultLibrary] len=3 "int"
// byte_strings.baml:24:5 (keyword) len=3 "let"
// byte_strings.baml:24:9 (variable) [declaration] len=4 "data"
// byte_strings.baml:24:14 (operator) len=1 "="
// byte_strings.baml:24:16 (string) len=2 "b\""
// byte_strings.baml:24:18 (escapeSequence) len=4 "\\x41"
// byte_strings.baml:24:22 (escapeSequence) len=4 "\\x42"
// byte_strings.baml:24:26 (escapeSequence) len=4 "\\x43"
// byte_strings.baml:24:30 (string) len=1 "\""
// byte_strings.baml:25:5 (variable) len=4 "data"
// byte_strings.baml:25:10 (method) len=2 "at"
// byte_strings.baml:25:13 (number) len=1 "0"
// byte_strings.baml:28:1 (comment) len=30 "// Byte strings in expressions"
// byte_strings.baml:29:1 (keyword) len=8 "function"
// byte_strings.baml:29:10 (function) [declaration] len=21 "ByteStringInCondition"
// byte_strings.baml:29:37 (type) [defaultLibrary] len=3 "int"
// byte_strings.baml:30:5 (keyword) len=2 "if"
// byte_strings.baml:30:9 (string) len=6 "b\"abc\""
// byte_strings.baml:30:16 (method) len=6 "length"
// byte_strings.baml:30:25 (operator) len=1 ">"
// byte_strings.baml:30:27 (number) len=1 "0"
// byte_strings.baml:31:9 (number) len=1 "1"
// byte_strings.baml:32:7 (keyword) len=4 "else"
// byte_strings.baml:33:9 (number) len=1 "0"
// byte_strings.baml:37:1 (keyword) len=8 "function"
// byte_strings.baml:37:10 (function) [declaration] len=18 "ByteStringEquality"
// byte_strings.baml:37:34 (type) [defaultLibrary] len=4 "bool"
// byte_strings.baml:38:5 (string) len=6 "b\"abc\""
// byte_strings.baml:38:12 (operator) len=2 "=="
// byte_strings.baml:38:15 (string) len=6 "b\"abc\""
// byte_strings.baml:41:1 (comment) len=35 "// Byte string as function argument"
// byte_strings.baml:42:1 (keyword) len=8 "function"
// byte_strings.baml:42:10 (function) [declaration] len=23 "ByteStringPassAndReturn"
// byte_strings.baml:42:34 (parameter) [declaration] len=4 "data"
// byte_strings.baml:42:40 (type) [defaultLibrary] len=10 "uint8array"
// byte_strings.baml:42:55 (type) [defaultLibrary] len=3 "int"
// byte_strings.baml:43:5 (parameter) len=4 "data"
// byte_strings.baml:43:10 (method) len=6 "length"
// byte_strings.baml:46:1 (comment) len=36 "// Byte string index write (mutable)"
// byte_strings.baml:47:1 (keyword) len=8 "function"
// byte_strings.baml:47:10 (function) [declaration] len=16 "ByteStringMutate"
// byte_strings.baml:47:32 (type) [defaultLibrary] len=3 "int"
// byte_strings.baml:48:5 (keyword) len=3 "let"
// byte_strings.baml:48:9 (variable) [declaration] len=4 "data"
// byte_strings.baml:48:14 (operator) len=1 "="
// byte_strings.baml:48:16 (string) len=2 "b\""
// byte_strings.baml:48:18 (escapeSequence) len=4 "\\x00"
// byte_strings.baml:48:22 (escapeSequence) len=4 "\\x00"
// byte_strings.baml:48:26 (escapeSequence) len=4 "\\x00"
// byte_strings.baml:48:30 (string) len=1 "\""
// byte_strings.baml:49:5 (variable) len=4 "data"
// byte_strings.baml:49:10 (number) len=1 "0"
// byte_strings.baml:49:13 (operator) len=1 "="
// byte_strings.baml:49:15 (number) len=2 "42"
// byte_strings.baml:50:5 (variable) len=4 "data"
// byte_strings.baml:50:10 (method) len=2 "at"
// byte_strings.baml:50:13 (number) len=1 "0"
// byte_strings.baml:53:1 (comment) len=42 "// Direct index read (data[0], not .at(0))"
// byte_strings.baml:54:1 (keyword) len=8 "function"
// byte_strings.baml:54:10 (function) [declaration] len=25 "ByteStringDirectIndexRead"
// byte_strings.baml:54:41 (type) [defaultLibrary] len=3 "int"
// byte_strings.baml:55:5 (keyword) len=3 "let"
// byte_strings.baml:55:9 (variable) [declaration] len=4 "data"
// byte_strings.baml:55:14 (operator) len=1 "="
// byte_strings.baml:55:16 (string) len=2 "b\""
// byte_strings.baml:55:18 (escapeSequence) len=4 "\\x41"
// byte_strings.baml:55:22 (escapeSequence) len=4 "\\x42"
// byte_strings.baml:55:26 (escapeSequence) len=4 "\\x43"
// byte_strings.baml:55:30 (string) len=1 "\""
// byte_strings.baml:56:5 (variable) len=4 "data"
// byte_strings.baml:56:10 (number) len=1 "0"
// byte_strings.baml:59:1 (comment) len=48 "// Byte string literal as function call argument"
// byte_strings.baml:60:1 (keyword) len=8 "function"
// byte_strings.baml:60:10 (function) [declaration] len=22 "ByteStringLiteralAsArg"
// byte_strings.baml:60:38 (type) [defaultLibrary] len=3 "int"
// byte_strings.baml:61:5 (function) len=23 "ByteStringPassAndReturn"
// byte_strings.baml:61:29 (string) len=6 "b\"abc\""
