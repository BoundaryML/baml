function FindFirst() -> int {
  let result = 0;
  while (true) {
    if (result == 5) {
      break;
    }
    result = result + 1;
  }
  result
}

function SumOdds() -> int {
  let sum = 0;
  for (let i = 0; i < 10; i += 1) {
    if (i % 2 == 0) {
      continue;
    }
    sum = sum + i;
  }
  sum
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- semantic_tokens
// break_continue.baml:1:1 (keyword) len=8 "function"
// break_continue.baml:1:10 (function) len=9 "FindFirst"
// break_continue.baml:1:25 (type) len=3 "int"
// break_continue.baml:2:3 (keyword) len=3 "let"
// break_continue.baml:2:7 (variable) len=6 "result"
// break_continue.baml:2:14 (operator) len=1 "="
// break_continue.baml:2:16 (number) len=1 "0"
// break_continue.baml:3:3 (keyword) len=5 "while"
// break_continue.baml:4:5 (keyword) len=2 "if"
// break_continue.baml:4:9 (variable) len=6 "result"
// break_continue.baml:4:16 (operator) len=2 "=="
// break_continue.baml:4:19 (number) len=1 "5"
// break_continue.baml:5:7 (keyword) len=5 "break"
// break_continue.baml:7:5 (variable) len=6 "result"
// break_continue.baml:7:12 (operator) len=1 "="
// break_continue.baml:7:14 (variable) len=6 "result"
// break_continue.baml:7:21 (operator) len=1 "+"
// break_continue.baml:7:23 (number) len=1 "1"
// break_continue.baml:9:3 (variable) len=6 "result"
// break_continue.baml:12:1 (keyword) len=8 "function"
// break_continue.baml:12:10 (function) len=7 "SumOdds"
// break_continue.baml:12:23 (type) len=3 "int"
// break_continue.baml:13:3 (keyword) len=3 "let"
// break_continue.baml:13:7 (variable) len=3 "sum"
// break_continue.baml:13:11 (operator) len=1 "="
// break_continue.baml:13:13 (number) len=1 "0"
// break_continue.baml:14:3 (keyword) len=3 "for"
// break_continue.baml:14:8 (keyword) len=3 "let"
// break_continue.baml:14:12 (variable) len=1 "i"
// break_continue.baml:14:14 (operator) len=1 "="
// break_continue.baml:14:16 (number) len=1 "0"
// break_continue.baml:14:19 (variable) len=1 "i"
// break_continue.baml:14:21 (operator) len=1 "<"
// break_continue.baml:14:23 (number) len=2 "10"
// break_continue.baml:14:27 (variable) len=1 "i"
// break_continue.baml:14:29 (operator) len=2 "+="
// break_continue.baml:14:32 (number) len=1 "1"
// break_continue.baml:15:5 (keyword) len=2 "if"
// break_continue.baml:15:9 (variable) len=1 "i"
// break_continue.baml:15:11 (operator) len=1 "%"
// break_continue.baml:15:13 (number) len=1 "2"
// break_continue.baml:15:15 (operator) len=2 "=="
// break_continue.baml:15:18 (number) len=1 "0"
// break_continue.baml:16:7 (keyword) len=8 "continue"
// break_continue.baml:18:5 (variable) len=3 "sum"
// break_continue.baml:18:9 (operator) len=1 "="
// break_continue.baml:18:11 (variable) len=3 "sum"
// break_continue.baml:18:15 (operator) len=1 "+"
// break_continue.baml:18:17 (variable) len=1 "i"
// break_continue.baml:20:3 (variable) len=3 "sum"
