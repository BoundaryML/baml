function CStyleLoop() -> int {
  let sum = 0;
  for (let i = 0; i < 10; i += 1) {
    sum = sum + i;
  }
  sum
}

function ForInLoop() -> int {
  let sum = 0;
  for (let x in [1, 2, 3]) {
    sum = sum + x;
  }
  sum
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- semantic_tokens
// for_loops.baml:1:1 (keyword) len=8 "function"
// for_loops.baml:1:10 (function) len=10 "CStyleLoop"
// for_loops.baml:1:26 (type) len=3 "int"
// for_loops.baml:2:3 (keyword) len=3 "let"
// for_loops.baml:2:7 (variable) len=3 "sum"
// for_loops.baml:2:11 (operator) len=1 "="
// for_loops.baml:2:13 (number) len=1 "0"
// for_loops.baml:3:3 (keyword) len=3 "for"
// for_loops.baml:3:8 (keyword) len=3 "let"
// for_loops.baml:3:12 (variable) len=1 "i"
// for_loops.baml:3:14 (operator) len=1 "="
// for_loops.baml:3:16 (number) len=1 "0"
// for_loops.baml:3:19 (variable) len=1 "i"
// for_loops.baml:3:21 (operator) len=1 "<"
// for_loops.baml:3:23 (number) len=2 "10"
// for_loops.baml:3:27 (variable) len=1 "i"
// for_loops.baml:3:29 (operator) len=2 "+="
// for_loops.baml:3:32 (number) len=1 "1"
// for_loops.baml:4:5 (variable) len=3 "sum"
// for_loops.baml:4:9 (operator) len=1 "="
// for_loops.baml:4:11 (variable) len=3 "sum"
// for_loops.baml:4:15 (operator) len=1 "+"
// for_loops.baml:4:17 (variable) len=1 "i"
// for_loops.baml:6:3 (variable) len=3 "sum"
// for_loops.baml:9:1 (keyword) len=8 "function"
// for_loops.baml:9:10 (function) len=9 "ForInLoop"
// for_loops.baml:9:25 (type) len=3 "int"
// for_loops.baml:10:3 (keyword) len=3 "let"
// for_loops.baml:10:7 (variable) len=3 "sum"
// for_loops.baml:10:11 (operator) len=1 "="
// for_loops.baml:10:13 (number) len=1 "0"
// for_loops.baml:11:3 (keyword) len=3 "for"
// for_loops.baml:11:8 (keyword) len=3 "let"
// for_loops.baml:11:12 (variable) len=1 "x"
// for_loops.baml:11:14 (keyword) len=2 "in"
// for_loops.baml:11:18 (number) len=1 "1"
// for_loops.baml:11:21 (number) len=1 "2"
// for_loops.baml:11:24 (number) len=1 "3"
// for_loops.baml:12:5 (variable) len=3 "sum"
// for_loops.baml:12:9 (operator) len=1 "="
// for_loops.baml:12:11 (variable) len=3 "sum"
// for_loops.baml:12:15 (operator) len=1 "+"
// for_loops.baml:12:17 (variable) len=1 "x"
// for_loops.baml:14:3 (variable) len=3 "sum"
