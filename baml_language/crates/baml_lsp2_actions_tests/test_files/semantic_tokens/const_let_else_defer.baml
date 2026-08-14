class User {
  name string
}

function BindingAndDeferForms(user: User, value: int | string, items: int[]) -> int {
  const x = 1;
  const y: int = x;
  const _ = y;

  let v: int = value else { return 0; };

  defer {
    let cleanup = v;
    cleanup;
  }

  let observed = v;

  if const narrowed: int = value {
    observed += narrowed;
  } else {
    observed += 0;
  };

  while const loop_value: int = value {
    observed += loop_value;
    break;
  }

  for (const i = 0; i < 3; i += 1) {
    observed += i;
  }

  for (const item in items) {
    observed += item;
  }

  for const item in items {
    observed += item;
  }

  observed
}

//----
//- diagnostics
// E0010
//
//   ⚠ `const` is currently treated like `let`; BAML does not enforce immutability yet. Use `let` for current BAML semantics.
//    ╭─[const_let_else_defer.baml:6:3]
//  6 │   const x = 1;
//    ·   ──┬──
//    ·     ╰── `const` behaves like `let` for now
//    ╰────
// E0010
//
//   ⚠ `const` is currently treated like `let`; BAML does not enforce immutability yet. Use `let` for current BAML semantics.
//    ╭─[const_let_else_defer.baml:7:3]
//  7 │   const y: int = x;
//    ·   ──┬──
//    ·     ╰── `const` behaves like `let` for now
//    ╰────
// E0010
//
//   ⚠ `const` is currently treated like `let`; BAML does not enforce immutability yet. Use `let` for current BAML semantics.
//    ╭─[const_let_else_defer.baml:8:3]
//  8 │   const _ = y;
//    ·   ──┬──
//    ·     ╰── `const` behaves like `let` for now
//    ╰────
// E0010
//
//   ⚠ `const` is currently treated like `let`; BAML does not enforce immutability yet. Use `let` for current BAML semantics.
//     ╭─[const_let_else_defer.baml:19:6]
//  19 │   if const narrowed: int = value {
//     ·      ──┬──
//     ·        ╰── `const` behaves like `let` for now
//     ╰────
// E0010
//
//   ⚠ `const` is currently treated like `let`; BAML does not enforce immutability yet. Use `let` for current BAML semantics.
//     ╭─[const_let_else_defer.baml:25:9]
//  25 │   while const loop_value: int = value {
//     ·         ──┬──
//     ·           ╰── `const` behaves like `let` for now
//     ╰────
// E0010
//
//   ⚠ `const` is currently treated like `let`; BAML does not enforce immutability yet. Use `let` for current BAML semantics.
//     ╭─[const_let_else_defer.baml:30:8]
//  30 │   for (const i = 0; i < 3; i += 1) {
//     ·        ──┬──
//     ·          ╰── `const` behaves like `let` for now
//     ╰────
// E0010
//
//   ⚠ `const` is currently treated like `let`; BAML does not enforce immutability yet. Use `let` for current BAML semantics.
//     ╭─[const_let_else_defer.baml:34:8]
//  34 │   for (const item in items) {
//     ·        ──┬──
//     ·          ╰── `const` behaves like `let` for now
//     ╰────
// E0010
//
//   ⚠ `const` is currently treated like `let`; BAML does not enforce immutability yet. Use `let` for current BAML semantics.
//     ╭─[const_let_else_defer.baml:38:7]
//  38 │   for const item in items {
//     ·       ──┬──
//     ·         ╰── `const` behaves like `let` for now
//     ╰────
//
//- semantic_tokens
// const_let_else_defer.baml:1:1 (keyword) len=5 "class"
// const_let_else_defer.baml:1:7 (class) [declaration] len=4 "User"
// const_let_else_defer.baml:2:3 (property) [declaration] len=4 "name"
// const_let_else_defer.baml:2:8 (type) [defaultLibrary] len=6 "string"
// const_let_else_defer.baml:5:1 (keyword) len=8 "function"
// const_let_else_defer.baml:5:10 (function) [declaration] len=20 "BindingAndDeferForms"
// const_let_else_defer.baml:5:31 (parameter) [declaration] len=4 "user"
// const_let_else_defer.baml:5:37 (class) len=4 "User"
// const_let_else_defer.baml:5:43 (parameter) [declaration] len=5 "value"
// const_let_else_defer.baml:5:50 (type) [defaultLibrary] len=3 "int"
// const_let_else_defer.baml:5:54 (operator) len=1 "|"
// const_let_else_defer.baml:5:56 (type) [defaultLibrary] len=6 "string"
// const_let_else_defer.baml:5:64 (parameter) [declaration] len=5 "items"
// const_let_else_defer.baml:5:71 (type) [defaultLibrary] len=3 "int"
// const_let_else_defer.baml:5:81 (type) [defaultLibrary] len=3 "int"
// const_let_else_defer.baml:6:3 (keyword) len=5 "const"
// const_let_else_defer.baml:6:9 (variable) [declaration] len=1 "x"
// const_let_else_defer.baml:6:11 (operator) len=1 "="
// const_let_else_defer.baml:6:13 (number) len=1 "1"
// const_let_else_defer.baml:7:3 (keyword) len=5 "const"
// const_let_else_defer.baml:7:9 (variable) [declaration] len=1 "y"
// const_let_else_defer.baml:7:12 (type) [defaultLibrary] len=3 "int"
// const_let_else_defer.baml:7:16 (operator) len=1 "="
// const_let_else_defer.baml:7:18 (variable) len=1 "x"
// const_let_else_defer.baml:8:3 (keyword) len=5 "const"
// const_let_else_defer.baml:8:11 (operator) len=1 "="
// const_let_else_defer.baml:8:13 (variable) len=1 "y"
// const_let_else_defer.baml:10:3 (keyword) len=3 "let"
// const_let_else_defer.baml:10:7 (variable) [declaration] len=1 "v"
// const_let_else_defer.baml:10:10 (type) [defaultLibrary] len=3 "int"
// const_let_else_defer.baml:10:14 (operator) len=1 "="
// const_let_else_defer.baml:10:16 (parameter) len=5 "value"
// const_let_else_defer.baml:10:22 (keyword) len=4 "else"
// const_let_else_defer.baml:10:29 (keyword) len=6 "return"
// const_let_else_defer.baml:10:36 (number) len=1 "0"
// const_let_else_defer.baml:12:3 (keyword) len=5 "defer"
// const_let_else_defer.baml:13:5 (keyword) len=3 "let"
// const_let_else_defer.baml:13:9 (variable) [declaration] len=7 "cleanup"
// const_let_else_defer.baml:13:17 (operator) len=1 "="
// const_let_else_defer.baml:13:19 (variable) len=1 "v"
// const_let_else_defer.baml:14:5 (variable) len=7 "cleanup"
// const_let_else_defer.baml:17:3 (keyword) len=3 "let"
// const_let_else_defer.baml:17:7 (variable) [declaration] len=8 "observed"
// const_let_else_defer.baml:17:16 (operator) len=1 "="
// const_let_else_defer.baml:17:18 (variable) len=1 "v"
// const_let_else_defer.baml:19:3 (keyword) len=2 "if"
// const_let_else_defer.baml:19:6 (keyword) len=5 "const"
// const_let_else_defer.baml:19:12 (variable) [declaration] len=8 "narrowed"
// const_let_else_defer.baml:19:22 (type) [defaultLibrary] len=3 "int"
// const_let_else_defer.baml:19:26 (operator) len=1 "="
// const_let_else_defer.baml:19:28 (parameter) len=5 "value"
// const_let_else_defer.baml:20:5 (variable) len=8 "observed"
// const_let_else_defer.baml:20:14 (operator) len=2 "+="
// const_let_else_defer.baml:20:17 (variable) len=8 "narrowed"
// const_let_else_defer.baml:21:5 (keyword) len=4 "else"
// const_let_else_defer.baml:22:5 (variable) len=8 "observed"
// const_let_else_defer.baml:22:14 (operator) len=2 "+="
// const_let_else_defer.baml:22:17 (number) len=1 "0"
// const_let_else_defer.baml:25:3 (keyword) len=5 "while"
// const_let_else_defer.baml:25:9 (keyword) len=5 "const"
// const_let_else_defer.baml:25:15 (variable) [declaration] len=10 "loop_value"
// const_let_else_defer.baml:25:27 (type) [defaultLibrary] len=3 "int"
// const_let_else_defer.baml:25:31 (operator) len=1 "="
// const_let_else_defer.baml:25:33 (parameter) len=5 "value"
// const_let_else_defer.baml:26:5 (variable) len=8 "observed"
// const_let_else_defer.baml:26:14 (operator) len=2 "+="
// const_let_else_defer.baml:26:17 (variable) len=10 "loop_value"
// const_let_else_defer.baml:27:5 (keyword) len=5 "break"
// const_let_else_defer.baml:30:3 (keyword) len=3 "for"
// const_let_else_defer.baml:30:8 (keyword) len=5 "const"
// const_let_else_defer.baml:30:14 (variable) [declaration] len=1 "i"
// const_let_else_defer.baml:30:16 (operator) len=1 "="
// const_let_else_defer.baml:30:18 (number) len=1 "0"
// const_let_else_defer.baml:30:21 (variable) len=1 "i"
// const_let_else_defer.baml:30:23 (operator) len=1 "<"
// const_let_else_defer.baml:30:25 (number) len=1 "3"
// const_let_else_defer.baml:30:28 (variable) len=1 "i"
// const_let_else_defer.baml:30:30 (operator) len=2 "+="
// const_let_else_defer.baml:30:33 (number) len=1 "1"
// const_let_else_defer.baml:31:5 (variable) len=8 "observed"
// const_let_else_defer.baml:31:14 (operator) len=2 "+="
// const_let_else_defer.baml:31:17 (variable) len=1 "i"
// const_let_else_defer.baml:34:3 (keyword) len=3 "for"
// const_let_else_defer.baml:34:8 (keyword) len=5 "const"
// const_let_else_defer.baml:34:14 (variable) [declaration] len=4 "item"
// const_let_else_defer.baml:34:19 (keyword) len=2 "in"
// const_let_else_defer.baml:34:22 (parameter) len=5 "items"
// const_let_else_defer.baml:35:5 (variable) len=8 "observed"
// const_let_else_defer.baml:35:14 (operator) len=2 "+="
// const_let_else_defer.baml:35:17 (variable) len=4 "item"
// const_let_else_defer.baml:38:3 (keyword) len=3 "for"
// const_let_else_defer.baml:38:7 (keyword) len=5 "const"
// const_let_else_defer.baml:38:13 (variable) [declaration] len=4 "item"
// const_let_else_defer.baml:38:18 (keyword) len=2 "in"
// const_let_else_defer.baml:38:21 (parameter) len=5 "items"
// const_let_else_defer.baml:39:5 (variable) len=8 "observed"
// const_let_else_defer.baml:39:14 (operator) len=2 "+="
// const_let_else_defer.baml:39:17 (variable) len=4 "item"
// const_let_else_defer.baml:42:3 (variable) len=8 "observed"
