function Abs(x: int) -> int {
  if (x < 0) {
    -x
  } else {
    x
  }
}

function Clamp(x: int) -> int {
  let result = if (x < 0) {
    0
  } else if (x > 100) {
    100
  } else {
    x
  };
  result
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- semantic_tokens
// if_else.baml:1:1 (keyword) len=8 "function"
// if_else.baml:1:10 (function) len=3 "Abs"
// if_else.baml:1:14 (parameter) len=1 "x"
// if_else.baml:1:17 (type) len=3 "int"
// if_else.baml:1:25 (type) len=3 "int"
// if_else.baml:2:3 (keyword) len=2 "if"
// if_else.baml:2:7 (parameter) len=1 "x"
// if_else.baml:2:9 (operator) len=1 "<"
// if_else.baml:2:11 (number) len=1 "0"
// if_else.baml:3:5 (operator) len=1 "-"
// if_else.baml:3:6 (parameter) len=1 "x"
// if_else.baml:4:5 (keyword) len=4 "else"
// if_else.baml:5:5 (parameter) len=1 "x"
// if_else.baml:9:1 (keyword) len=8 "function"
// if_else.baml:9:10 (function) len=5 "Clamp"
// if_else.baml:9:16 (parameter) len=1 "x"
// if_else.baml:9:19 (type) len=3 "int"
// if_else.baml:9:27 (type) len=3 "int"
// if_else.baml:10:3 (keyword) len=3 "let"
// if_else.baml:10:7 (variable) len=6 "result"
// if_else.baml:10:14 (operator) len=1 "="
// if_else.baml:10:16 (keyword) len=2 "if"
// if_else.baml:10:20 (parameter) len=1 "x"
// if_else.baml:10:22 (operator) len=1 "<"
// if_else.baml:10:24 (number) len=1 "0"
// if_else.baml:11:5 (number) len=1 "0"
// if_else.baml:12:5 (keyword) len=4 "else"
// if_else.baml:12:10 (keyword) len=2 "if"
// if_else.baml:12:14 (parameter) len=1 "x"
// if_else.baml:12:16 (operator) len=1 ">"
// if_else.baml:12:18 (number) len=3 "100"
// if_else.baml:13:5 (number) len=3 "100"
// if_else.baml:14:5 (keyword) len=4 "else"
// if_else.baml:15:5 (parameter) len=1 "x"
// if_else.baml:17:3 (variable) len=6 "result"
