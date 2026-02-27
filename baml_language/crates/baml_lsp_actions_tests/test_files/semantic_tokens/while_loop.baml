function GCD(a: int, b: int) -> int {
  while (a != b) {
    if (a > b) {
      a = a - b;
    } else {
      b = b - a;
    }
  }
  a
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- semantic_tokens
// while_loop.baml:1:1 (keyword) len=8 "function"
// while_loop.baml:1:10 (function) len=3 "GCD"
// while_loop.baml:1:14 (parameter) len=1 "a"
// while_loop.baml:1:17 (type) len=3 "int"
// while_loop.baml:1:22 (parameter) len=1 "b"
// while_loop.baml:1:25 (type) len=3 "int"
// while_loop.baml:1:33 (type) len=3 "int"
// while_loop.baml:2:3 (keyword) len=5 "while"
// while_loop.baml:2:10 (parameter) len=1 "a"
// while_loop.baml:2:12 (operator) len=2 "!="
// while_loop.baml:2:15 (parameter) len=1 "b"
// while_loop.baml:3:5 (keyword) len=2 "if"
// while_loop.baml:3:9 (parameter) len=1 "a"
// while_loop.baml:3:11 (operator) len=1 ">"
// while_loop.baml:3:13 (parameter) len=1 "b"
// while_loop.baml:4:7 (parameter) len=1 "a"
// while_loop.baml:4:9 (operator) len=1 "="
// while_loop.baml:4:11 (parameter) len=1 "a"
// while_loop.baml:4:13 (operator) len=1 "-"
// while_loop.baml:4:15 (parameter) len=1 "b"
// while_loop.baml:5:7 (keyword) len=4 "else"
// while_loop.baml:6:7 (parameter) len=1 "b"
// while_loop.baml:6:9 (operator) len=1 "="
// while_loop.baml:6:11 (parameter) len=1 "b"
// while_loop.baml:6:13 (operator) len=1 "-"
// while_loop.baml:6:15 (parameter) len=1 "a"
// while_loop.baml:9:3 (parameter) len=1 "a"
