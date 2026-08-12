// `else if let` and `else if` chains.
class Ok {
  value string
}
class Err {
  message string
}
class Empty {}

// Plain `else if` after `if let`. The `is Empty { ... }` form is no
// longer ambiguous with the then-block in condition position — the
// parser suppresses destructure patterns there (Rust-style restriction).
function MixedChainPlain(r: Ok | Err | Empty) -> string {
  if let o: Ok = r {
    o.value
  } else if r is Empty {
    "empty"
  } else {
    "other"
  }
}

// `else if let` chain — each branch binds its own name.
function IfLetChain(r: Ok | Err | Empty) -> string {
  if let o: Ok = r {
    o.value
  } else if let e: Err = r {
    e.message
  } else {
    "empty"
  }
}

// `else if let` without a final else.
function IfLetChainNoElse(r: Ok | Err) -> string {
  if let o: Ok = r {
    return o.value;
  } else if let e: Err = r {
    return e.message;
  }
  "unreachable"
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- semantic_tokens
// if_let_chain.baml:1:1 (comment) len=38 "// `else if let` and `else if` chains."
// if_let_chain.baml:2:1 (keyword) len=5 "class"
// if_let_chain.baml:2:7 (class) [declaration] len=2 "Ok"
// if_let_chain.baml:3:3 (property) [declaration] len=5 "value"
// if_let_chain.baml:3:9 (type) [defaultLibrary] len=6 "string"
// if_let_chain.baml:5:1 (keyword) len=5 "class"
// if_let_chain.baml:5:7 (class) [declaration] len=3 "Err"
// if_let_chain.baml:6:3 (property) [declaration] len=7 "message"
// if_let_chain.baml:6:11 (type) [defaultLibrary] len=6 "string"
// if_let_chain.baml:8:1 (keyword) len=5 "class"
// if_let_chain.baml:8:7 (class) [declaration] len=5 "Empty"
// if_let_chain.baml:10:1 (comment) len=68 "// Plain `else if` after `if let`. The `is Empty { ... }` form is no"
// if_let_chain.baml:11:1 (comment) len=69 "// longer ambiguous with the then-block in condition position — the"
// if_let_chain.baml:12:1 (comment) len=73 "// parser suppresses destructure patterns there (Rust-style restriction)."
// if_let_chain.baml:13:1 (keyword) len=8 "function"
// if_let_chain.baml:13:10 (function) [declaration] len=15 "MixedChainPlain"
// if_let_chain.baml:13:26 (parameter) [declaration] len=1 "r"
// if_let_chain.baml:13:29 (class) len=2 "Ok"
// if_let_chain.baml:13:32 (operator) len=1 "|"
// if_let_chain.baml:13:34 (class) len=3 "Err"
// if_let_chain.baml:13:38 (operator) len=1 "|"
// if_let_chain.baml:13:40 (class) len=5 "Empty"
// if_let_chain.baml:13:50 (type) [defaultLibrary] len=6 "string"
// if_let_chain.baml:14:3 (keyword) len=2 "if"
// if_let_chain.baml:14:6 (keyword) len=3 "let"
// if_let_chain.baml:14:10 (variable) [declaration] len=1 "o"
// if_let_chain.baml:14:13 (class) len=2 "Ok"
// if_let_chain.baml:14:16 (operator) len=1 "="
// if_let_chain.baml:14:18 (parameter) len=1 "r"
// if_let_chain.baml:15:5 (variable) len=1 "o"
// if_let_chain.baml:15:7 (property) len=5 "value"
// if_let_chain.baml:16:5 (keyword) len=4 "else"
// if_let_chain.baml:16:10 (keyword) len=2 "if"
// if_let_chain.baml:16:13 (parameter) len=1 "r"
// if_let_chain.baml:16:15 (keyword) len=2 "is"
// if_let_chain.baml:16:18 (class) len=5 "Empty"
// if_let_chain.baml:17:5 (string) len=7 "\"empty\""
// if_let_chain.baml:18:5 (keyword) len=4 "else"
// if_let_chain.baml:19:5 (string) len=7 "\"other\""
// if_let_chain.baml:23:1 (comment) len=58 "// `else if let` chain — each branch binds its own name."
// if_let_chain.baml:24:1 (keyword) len=8 "function"
// if_let_chain.baml:24:10 (function) [declaration] len=10 "IfLetChain"
// if_let_chain.baml:24:21 (parameter) [declaration] len=1 "r"
// if_let_chain.baml:24:24 (class) len=2 "Ok"
// if_let_chain.baml:24:27 (operator) len=1 "|"
// if_let_chain.baml:24:29 (class) len=3 "Err"
// if_let_chain.baml:24:33 (operator) len=1 "|"
// if_let_chain.baml:24:35 (class) len=5 "Empty"
// if_let_chain.baml:24:45 (type) [defaultLibrary] len=6 "string"
// if_let_chain.baml:25:3 (keyword) len=2 "if"
// if_let_chain.baml:25:6 (keyword) len=3 "let"
// if_let_chain.baml:25:10 (variable) [declaration] len=1 "o"
// if_let_chain.baml:25:13 (class) len=2 "Ok"
// if_let_chain.baml:25:16 (operator) len=1 "="
// if_let_chain.baml:25:18 (parameter) len=1 "r"
// if_let_chain.baml:26:5 (variable) len=1 "o"
// if_let_chain.baml:26:7 (property) len=5 "value"
// if_let_chain.baml:27:5 (keyword) len=4 "else"
// if_let_chain.baml:27:10 (keyword) len=2 "if"
// if_let_chain.baml:27:13 (keyword) len=3 "let"
// if_let_chain.baml:27:17 (variable) [declaration] len=1 "e"
// if_let_chain.baml:27:20 (class) len=3 "Err"
// if_let_chain.baml:27:24 (operator) len=1 "="
// if_let_chain.baml:27:26 (parameter) len=1 "r"
// if_let_chain.baml:28:5 (variable) len=1 "e"
// if_let_chain.baml:28:7 (property) len=7 "message"
// if_let_chain.baml:29:5 (keyword) len=4 "else"
// if_let_chain.baml:30:5 (string) len=7 "\"empty\""
// if_let_chain.baml:34:1 (comment) len=38 "// `else if let` without a final else."
// if_let_chain.baml:35:1 (keyword) len=8 "function"
// if_let_chain.baml:35:10 (function) [declaration] len=16 "IfLetChainNoElse"
// if_let_chain.baml:35:27 (parameter) [declaration] len=1 "r"
// if_let_chain.baml:35:30 (class) len=2 "Ok"
// if_let_chain.baml:35:33 (operator) len=1 "|"
// if_let_chain.baml:35:35 (class) len=3 "Err"
// if_let_chain.baml:35:43 (type) [defaultLibrary] len=6 "string"
// if_let_chain.baml:36:3 (keyword) len=2 "if"
// if_let_chain.baml:36:6 (keyword) len=3 "let"
// if_let_chain.baml:36:10 (variable) [declaration] len=1 "o"
// if_let_chain.baml:36:13 (class) len=2 "Ok"
// if_let_chain.baml:36:16 (operator) len=1 "="
// if_let_chain.baml:36:18 (parameter) len=1 "r"
// if_let_chain.baml:37:5 (keyword) len=6 "return"
// if_let_chain.baml:37:12 (variable) len=1 "o"
// if_let_chain.baml:37:14 (property) len=5 "value"
// if_let_chain.baml:38:5 (keyword) len=4 "else"
// if_let_chain.baml:38:10 (keyword) len=2 "if"
// if_let_chain.baml:38:13 (keyword) len=3 "let"
// if_let_chain.baml:38:17 (variable) [declaration] len=1 "e"
// if_let_chain.baml:38:20 (class) len=3 "Err"
// if_let_chain.baml:38:24 (operator) len=1 "="
// if_let_chain.baml:38:26 (parameter) len=1 "r"
// if_let_chain.baml:39:5 (keyword) len=6 "return"
// if_let_chain.baml:39:12 (variable) len=1 "e"
// if_let_chain.baml:39:14 (property) len=7 "message"
// if_let_chain.baml:41:3 (string) len=13 "\"unreachable\""
