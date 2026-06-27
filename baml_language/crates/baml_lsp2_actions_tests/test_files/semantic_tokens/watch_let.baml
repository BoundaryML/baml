function branch_locals(b: bool) -> int {
  if (b) {
    let a = 1;
    a
  } else {
    let a = 2;
    a
  }
}

function same_scope_shadow() -> int {
  let x = 1;
  let x = 2;
  x
}

function initializer_uses_previous() -> int {
  let x = 1;
  let x = x + 1;
  x
}

function shadow_param(x: int) -> int {
  let x = x + 1;
  x
}

function outer_restored() -> int {
  let x = 1;
  {
    let x = 2;
  };
  x
}

function declared_type_restored() -> string {
  let x: string = "outer";
  {
    let x: int = 1;
  };
  x
}

function for_loop_restores_outer() -> int {
  let x = 1;
  for (let x in [2, 3]) {
    x;
  };
  x
}

function watch_block_cleanup() -> int {
  watch let x = 1;
  {
    watch let x = 2;
    x;
  };
  x
}

function nested_outer_restored() -> int {
  let x = 1;
  {
    let x = 2;
    {
      let x = 3;
      x;
    };
    x;
  };
  x
}

function capture_before_after_shadow() -> int {
  let x = 1;
  let g = () -> int { x };
  let x = 2;
  let f = () -> int { x };
  g() * 10 + f()
}

function nested_lambda_capture() -> int {
  let x = 7;
  let outer = () -> int {
    let inner = () -> int { x };
    inner()
  };
  outer()
}

// A `let` declared inside a `while` body must NOT leak to the enclosing
// scope. The inner `x = 99` shadows the outer `x` for the duration of the
// body only — after the loop, `x` resolves to the outermost binding.
function while_body_restores_outer() -> int {
  let x = 1;
  let once = true;
  while (once) {
    let x = 99;
    once = false;
  };
  x
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- semantic_tokens
// watch_let.baml:1:1 (keyword) len=8 "function"
// watch_let.baml:1:10 (function) [declaration] len=13 "branch_locals"
// watch_let.baml:1:24 (parameter) [declaration] len=1 "b"
// watch_let.baml:1:27 (type) [defaultLibrary] len=4 "bool"
// watch_let.baml:1:36 (type) [defaultLibrary] len=3 "int"
// watch_let.baml:2:3 (keyword) len=2 "if"
// watch_let.baml:2:7 (parameter) len=1 "b"
// watch_let.baml:3:5 (keyword) len=3 "let"
// watch_let.baml:3:9 (variable) [declaration] len=1 "a"
// watch_let.baml:3:11 (operator) len=1 "="
// watch_let.baml:3:13 (number) len=1 "1"
// watch_let.baml:4:5 (variable) len=1 "a"
// watch_let.baml:5:5 (keyword) len=4 "else"
// watch_let.baml:6:5 (keyword) len=3 "let"
// watch_let.baml:6:9 (variable) [declaration] len=1 "a"
// watch_let.baml:6:11 (operator) len=1 "="
// watch_let.baml:6:13 (number) len=1 "2"
// watch_let.baml:7:5 (variable) len=1 "a"
// watch_let.baml:11:1 (keyword) len=8 "function"
// watch_let.baml:11:10 (function) [declaration] len=17 "same_scope_shadow"
// watch_let.baml:11:33 (type) [defaultLibrary] len=3 "int"
// watch_let.baml:12:3 (keyword) len=3 "let"
// watch_let.baml:12:7 (variable) [declaration] len=1 "x"
// watch_let.baml:12:9 (operator) len=1 "="
// watch_let.baml:12:11 (number) len=1 "1"
// watch_let.baml:13:3 (keyword) len=3 "let"
// watch_let.baml:13:7 (variable) [declaration] len=1 "x"
// watch_let.baml:13:9 (operator) len=1 "="
// watch_let.baml:13:11 (number) len=1 "2"
// watch_let.baml:14:3 (variable) len=1 "x"
// watch_let.baml:17:1 (keyword) len=8 "function"
// watch_let.baml:17:10 (function) [declaration] len=25 "initializer_uses_previous"
// watch_let.baml:17:41 (type) [defaultLibrary] len=3 "int"
// watch_let.baml:18:3 (keyword) len=3 "let"
// watch_let.baml:18:7 (variable) [declaration] len=1 "x"
// watch_let.baml:18:9 (operator) len=1 "="
// watch_let.baml:18:11 (number) len=1 "1"
// watch_let.baml:19:3 (keyword) len=3 "let"
// watch_let.baml:19:7 (variable) [declaration] len=1 "x"
// watch_let.baml:19:9 (operator) len=1 "="
// watch_let.baml:19:11 (variable) len=1 "x"
// watch_let.baml:19:13 (operator) len=1 "+"
// watch_let.baml:19:15 (number) len=1 "1"
// watch_let.baml:20:3 (variable) len=1 "x"
// watch_let.baml:23:1 (keyword) len=8 "function"
// watch_let.baml:23:10 (function) [declaration] len=12 "shadow_param"
// watch_let.baml:23:23 (parameter) [declaration] len=1 "x"
// watch_let.baml:23:26 (type) [defaultLibrary] len=3 "int"
// watch_let.baml:23:34 (type) [defaultLibrary] len=3 "int"
// watch_let.baml:24:3 (keyword) len=3 "let"
// watch_let.baml:24:7 (variable) [declaration] len=1 "x"
// watch_let.baml:24:9 (operator) len=1 "="
// watch_let.baml:24:11 (parameter) len=1 "x"
// watch_let.baml:24:13 (operator) len=1 "+"
// watch_let.baml:24:15 (number) len=1 "1"
// watch_let.baml:25:3 (variable) len=1 "x"
// watch_let.baml:28:1 (keyword) len=8 "function"
// watch_let.baml:28:10 (function) [declaration] len=14 "outer_restored"
// watch_let.baml:28:30 (type) [defaultLibrary] len=3 "int"
// watch_let.baml:29:3 (keyword) len=3 "let"
// watch_let.baml:29:7 (variable) [declaration] len=1 "x"
// watch_let.baml:29:9 (operator) len=1 "="
// watch_let.baml:29:11 (number) len=1 "1"
// watch_let.baml:31:5 (keyword) len=3 "let"
// watch_let.baml:31:9 (variable) [declaration] len=1 "x"
// watch_let.baml:31:11 (operator) len=1 "="
// watch_let.baml:31:13 (number) len=1 "2"
// watch_let.baml:33:3 (variable) len=1 "x"
// watch_let.baml:36:1 (keyword) len=8 "function"
// watch_let.baml:36:10 (function) [declaration] len=22 "declared_type_restored"
// watch_let.baml:36:38 (type) [defaultLibrary] len=6 "string"
// watch_let.baml:37:3 (keyword) len=3 "let"
// watch_let.baml:37:7 (variable) [declaration] len=1 "x"
// watch_let.baml:37:10 (type) [defaultLibrary] len=6 "string"
// watch_let.baml:37:17 (operator) len=1 "="
// watch_let.baml:37:19 (string) len=7 "\"outer\""
// watch_let.baml:39:5 (keyword) len=3 "let"
// watch_let.baml:39:9 (variable) [declaration] len=1 "x"
// watch_let.baml:39:12 (type) [defaultLibrary] len=3 "int"
// watch_let.baml:39:16 (operator) len=1 "="
// watch_let.baml:39:18 (number) len=1 "1"
// watch_let.baml:41:3 (variable) len=1 "x"
// watch_let.baml:44:1 (keyword) len=8 "function"
// watch_let.baml:44:10 (function) [declaration] len=23 "for_loop_restores_outer"
// watch_let.baml:44:39 (type) [defaultLibrary] len=3 "int"
// watch_let.baml:45:3 (keyword) len=3 "let"
// watch_let.baml:45:7 (variable) [declaration] len=1 "x"
// watch_let.baml:45:9 (operator) len=1 "="
// watch_let.baml:45:11 (number) len=1 "1"
// watch_let.baml:46:3 (keyword) len=3 "for"
// watch_let.baml:46:8 (keyword) len=3 "let"
// watch_let.baml:46:12 (variable) [declaration] len=1 "x"
// watch_let.baml:46:14 (keyword) len=2 "in"
// watch_let.baml:46:18 (number) len=1 "2"
// watch_let.baml:46:21 (number) len=1 "3"
// watch_let.baml:47:5 (variable) len=1 "x"
// watch_let.baml:49:3 (variable) len=1 "x"
// watch_let.baml:52:1 (keyword) len=8 "function"
// watch_let.baml:52:10 (function) [declaration] len=19 "watch_block_cleanup"
// watch_let.baml:52:35 (type) [defaultLibrary] len=3 "int"
// watch_let.baml:53:3 (keyword) len=5 "watch"
// watch_let.baml:53:9 (keyword) len=3 "let"
// watch_let.baml:53:13 (variable) [declaration] len=1 "x"
// watch_let.baml:53:15 (operator) len=1 "="
// watch_let.baml:53:17 (number) len=1 "1"
// watch_let.baml:55:5 (keyword) len=5 "watch"
// watch_let.baml:55:11 (keyword) len=3 "let"
// watch_let.baml:55:15 (variable) [declaration] len=1 "x"
// watch_let.baml:55:17 (operator) len=1 "="
// watch_let.baml:55:19 (number) len=1 "2"
// watch_let.baml:56:5 (variable) len=1 "x"
// watch_let.baml:58:3 (variable) len=1 "x"
// watch_let.baml:61:1 (keyword) len=8 "function"
// watch_let.baml:61:10 (function) [declaration] len=21 "nested_outer_restored"
// watch_let.baml:61:37 (type) [defaultLibrary] len=3 "int"
// watch_let.baml:62:3 (keyword) len=3 "let"
// watch_let.baml:62:7 (variable) [declaration] len=1 "x"
// watch_let.baml:62:9 (operator) len=1 "="
// watch_let.baml:62:11 (number) len=1 "1"
// watch_let.baml:64:5 (keyword) len=3 "let"
// watch_let.baml:64:9 (variable) [declaration] len=1 "x"
// watch_let.baml:64:11 (operator) len=1 "="
// watch_let.baml:64:13 (number) len=1 "2"
// watch_let.baml:66:7 (keyword) len=3 "let"
// watch_let.baml:66:11 (variable) [declaration] len=1 "x"
// watch_let.baml:66:13 (operator) len=1 "="
// watch_let.baml:66:15 (number) len=1 "3"
// watch_let.baml:67:7 (variable) len=1 "x"
// watch_let.baml:69:5 (variable) len=1 "x"
// watch_let.baml:71:3 (variable) len=1 "x"
// watch_let.baml:74:1 (keyword) len=8 "function"
// watch_let.baml:74:10 (function) [declaration] len=27 "capture_before_after_shadow"
// watch_let.baml:74:43 (type) [defaultLibrary] len=3 "int"
// watch_let.baml:75:3 (keyword) len=3 "let"
// watch_let.baml:75:7 (variable) [declaration] len=1 "x"
// watch_let.baml:75:9 (operator) len=1 "="
// watch_let.baml:75:11 (number) len=1 "1"
// watch_let.baml:76:3 (keyword) len=3 "let"
// watch_let.baml:76:7 (variable) [declaration] len=1 "g"
// watch_let.baml:76:9 (operator) len=1 "="
// watch_let.baml:76:17 (type) [defaultLibrary] len=3 "int"
// watch_let.baml:76:23 (variable) len=1 "x"
// watch_let.baml:77:3 (keyword) len=3 "let"
// watch_let.baml:77:7 (variable) [declaration] len=1 "x"
// watch_let.baml:77:9 (operator) len=1 "="
// watch_let.baml:77:11 (number) len=1 "2"
// watch_let.baml:78:3 (keyword) len=3 "let"
// watch_let.baml:78:7 (variable) [declaration] len=1 "f"
// watch_let.baml:78:9 (operator) len=1 "="
// watch_let.baml:78:17 (type) [defaultLibrary] len=3 "int"
// watch_let.baml:78:23 (variable) len=1 "x"
// watch_let.baml:79:3 (variable) len=1 "g"
// watch_let.baml:79:7 (operator) len=1 "*"
// watch_let.baml:79:9 (number) len=2 "10"
// watch_let.baml:79:12 (operator) len=1 "+"
// watch_let.baml:79:14 (variable) len=1 "f"
// watch_let.baml:82:1 (keyword) len=8 "function"
// watch_let.baml:82:10 (function) [declaration] len=21 "nested_lambda_capture"
// watch_let.baml:82:37 (type) [defaultLibrary] len=3 "int"
// watch_let.baml:83:3 (keyword) len=3 "let"
// watch_let.baml:83:7 (variable) [declaration] len=1 "x"
// watch_let.baml:83:9 (operator) len=1 "="
// watch_let.baml:83:11 (number) len=1 "7"
// watch_let.baml:84:3 (keyword) len=3 "let"
// watch_let.baml:84:7 (variable) [declaration] len=5 "outer"
// watch_let.baml:84:13 (operator) len=1 "="
// watch_let.baml:84:21 (type) [defaultLibrary] len=3 "int"
// watch_let.baml:85:5 (keyword) len=3 "let"
// watch_let.baml:85:9 (variable) [declaration] len=5 "inner"
// watch_let.baml:85:15 (operator) len=1 "="
// watch_let.baml:85:23 (type) [defaultLibrary] len=3 "int"
// watch_let.baml:85:29 (variable) len=1 "x"
// watch_let.baml:86:5 (variable) len=5 "inner"
// watch_let.baml:88:3 (variable) len=5 "outer"
// watch_let.baml:91:1 (comment) len=72 "// A `let` declared inside a `while` body must NOT leak to the enclosing"
// watch_let.baml:92:1 (comment) len=74 "// scope. The inner `x = 99` shadows the outer `x` for the duration of the"
// watch_let.baml:93:1 (comment) len=71 "// body only — after the loop, `x` resolves to the outermost binding."
// watch_let.baml:94:1 (keyword) len=8 "function"
// watch_let.baml:94:10 (function) [declaration] len=25 "while_body_restores_outer"
// watch_let.baml:94:41 (type) [defaultLibrary] len=3 "int"
// watch_let.baml:95:3 (keyword) len=3 "let"
// watch_let.baml:95:7 (variable) [declaration] len=1 "x"
// watch_let.baml:95:9 (operator) len=1 "="
// watch_let.baml:95:11 (number) len=1 "1"
// watch_let.baml:96:3 (keyword) len=3 "let"
// watch_let.baml:96:7 (variable) [declaration] len=4 "once"
// watch_let.baml:96:12 (operator) len=1 "="
// watch_let.baml:96:14 (keyword) len=4 "true"
// watch_let.baml:97:3 (keyword) len=5 "while"
// watch_let.baml:97:10 (variable) len=4 "once"
// watch_let.baml:98:5 (keyword) len=3 "let"
// watch_let.baml:98:9 (variable) [declaration] len=1 "x"
// watch_let.baml:98:11 (operator) len=1 "="
// watch_let.baml:98:13 (number) len=2 "99"
// watch_let.baml:99:5 (variable) len=4 "once"
// watch_let.baml:99:10 (operator) len=1 "="
// watch_let.baml:99:12 (keyword) len=5 "false"
// watch_let.baml:101:3 (variable) len=1 "x"
