function foo() -> int {

   // if

   if (true) { }

   if true {}

   if true) { }

   if (true { }

   // while

   while true {}

   while (true {}

   while true) {}

   while (true) {}

   // c-for, empty

   for ;; { }

   for (;; {}

   for ;;) {}

   for (;;) {}

   // c-for, trailing identifier, assign_op

   for (;;i += x) {}

   for (;;i += x {}

   for ;;i += x) {}

   for ;;i += x {}

	// c-for, trailing identifier, assign

   for (;;i = x) {}

   for (;;i = x {}

   for ;;i = x) {}

   for ;;i = x {}

   // iter-for, trailing identifier

   for let a in as {}

   for (let a in as {}

   for let a in as) {}

   for (let a in as) {}

}

// error: Error validating: expected parentheses around if expression condition
//   -->  parens.baml:7
//    | 
//  6 | 
//  7 |    if true {}
//    | 
// error: Error validating: expected opening parentheses for this closing parentheses
//   -->  parens.baml:9
//    | 
//  8 | 
//  9 |    if true) { }
//    | 
// error: Error validating: expected closing parentheses for this opening parentheses
//   -->  parens.baml:11
//    | 
// 10 | 
// 11 |    if (true { }
//    | 
// error: Error validating: expected parentheses around while loop condition
//   -->  parens.baml:15
//    | 
// 14 | 
// 15 |    while true {}
//    | 
// error: Error validating: expected closing parentheses for this opening parentheses
//   -->  parens.baml:17
//    | 
// 16 | 
// 17 |    while (true {}
//    | 
// error: Error validating: expected opening parentheses for this closing parentheses
//   -->  parens.baml:19
//    | 
// 18 | 
// 19 |    while true) {}
//    | 
// error: Error validating: expected parentheses around for loop header
//   -->  parens.baml:25
//    | 
// 24 | 
// 25 |    for ;; { }
//    | 
// error: Error validating: expected closing parentheses for this opening parentheses
//   -->  parens.baml:27
//    | 
// 26 | 
// 27 |    for (;; {}
//    | 
// error: Error validating: expected opening parentheses for this closing parentheses
//   -->  parens.baml:29
//    | 
// 28 | 
// 29 |    for ;;) {}
//    | 
// error: Error validating: expected closing parentheses for this opening parentheses
//   -->  parens.baml:37
//    | 
// 36 | 
// 37 |    for (;;i += x {}
//    | 
// error: Error validating: expected opening parentheses for this closing parentheses
//   -->  parens.baml:39
//    | 
// 38 | 
// 39 |    for ;;i += x) {}
//    | 
// error: Error validating: expected parentheses around for loop header
//   -->  parens.baml:41
//    | 
// 40 | 
// 41 |    for ;;i += x {}
//    | 
// error: Error validating: expected closing parentheses for this opening parentheses
//   -->  parens.baml:47
//    | 
// 46 | 
// 47 |    for (;;i = x {}
//    | 
// error: Error validating: expected opening parentheses for this closing parentheses
//   -->  parens.baml:49
//    | 
// 48 | 
// 49 |    for ;;i = x) {}
//    | 
// error: Error validating: expected parentheses around for loop header
//   -->  parens.baml:51
//    | 
// 50 | 
// 51 |    for ;;i = x {}
//    | 
// error: Error validating: expected parentheses around for loop header
//   -->  parens.baml:55
//    | 
// 54 | 
// 55 |    for let a in as {}
//    | 
// error: Error validating: expected closing parentheses for this opening parentheses
//   -->  parens.baml:57
//    | 
// 56 | 
// 57 |    for (let a in as {}
//    | 
// error: Error validating: expected opening parentheses for this closing parentheses
//   -->  parens.baml:59
//    | 
// 58 | 
// 59 |    for let a in as) {}
//    |

//----
//- diagnostics
// Error: Expected block after if condition, found If
//    ╭─[ 0:9:4 ]
//    │
//  9 │    if true) { }
//    │    ─┬  
//    │     ╰── Expected block after if condition, found If
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected block after if condition, found RParen
//    ╭─[ 0:9:11 ]
//    │
//  9 │    if true) { }
//    │           ┬  
//    │           ╰── Expected block after if condition, found RParen
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected expression, found RParen
//    ╭─[ 0:9:11 ]
//    │
//  9 │    if true) { }
//    │           ┬  
//    │           ╰── Expected expression, found RParen
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected RParen, found While
//     ╭─[ 0:15:4 ]
//     │
//  15 │    while true {}
//     │    ──┬──  
//     │      ╰──── Expected RParen, found While
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected block after if condition, found While
//     ╭─[ 0:15:4 ]
//     │
//  15 │    while true {}
//     │    ──┬──  
//     │      ╰──── Expected block after if condition, found While
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected block after while condition, found While
//     ╭─[ 0:17:4 ]
//     │
//  17 │    while (true {}
//     │    ──┬──  
//     │      ╰──── Expected block after while condition, found While
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected RParen, found While
//     ╭─[ 0:19:4 ]
//     │
//  19 │    while true) {}
//     │    ──┬──  
//     │      ╰──── Expected RParen, found While
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected block after while condition, found While
//     ╭─[ 0:19:4 ]
//     │
//  19 │    while true) {}
//     │    ──┬──  
//     │      ╰──── Expected block after while condition, found While
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected block after while condition, found RParen
//     ╭─[ 0:19:14 ]
//     │
//  19 │    while true) {}
//     │              ┬  
//     │              ╰── Expected block after while condition, found RParen
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected expression, found RParen
//     ╭─[ 0:19:14 ]
//     │
//  19 │    while true) {}
//     │              ┬  
//     │              ╰── Expected expression, found RParen
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected loop variable, found Semicolon
//     ╭─[ 0:25:8 ]
//     │
//  25 │    for ;; { }
//     │        ┬  
//     │        ╰── Expected loop variable, found Semicolon
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected In, found Semicolon
//     ╭─[ 0:25:8 ]
//     │
//  25 │    for ;; { }
//     │        ┬  
//     │        ╰── Expected In, found Semicolon
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected expression, found Semicolon
//     ╭─[ 0:25:8 ]
//     │
//  25 │    for ;; { }
//     │        ┬  
//     │        ╰── Expected expression, found Semicolon
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected block after for expression, found Semicolon
//     ╭─[ 0:25:9 ]
//     │
//  25 │    for ;; { }
//     │         ┬  
//     │         ╰── Expected block after for expression, found Semicolon
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected RParen, found For
//     ╭─[ 0:29:4 ]
//     │
//  29 │    for ;;) {}
//     │    ─┬─  
//     │     ╰─── Expected RParen, found For
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected block after for expression, found For
//     ╭─[ 0:29:4 ]
//     │
//  29 │    for ;;) {}
//     │    ─┬─  
//     │     ╰─── Expected block after for expression, found For
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected loop variable, found Semicolon
//     ╭─[ 0:29:8 ]
//     │
//  29 │    for ;;) {}
//     │        ┬  
//     │        ╰── Expected loop variable, found Semicolon
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected In, found Semicolon
//     ╭─[ 0:29:8 ]
//     │
//  29 │    for ;;) {}
//     │        ┬  
//     │        ╰── Expected In, found Semicolon
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected expression, found Semicolon
//     ╭─[ 0:29:8 ]
//     │
//  29 │    for ;;) {}
//     │        ┬  
//     │        ╰── Expected expression, found Semicolon
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected block after for expression, found Semicolon
//     ╭─[ 0:29:9 ]
//     │
//  29 │    for ;;) {}
//     │         ┬  
//     │         ╰── Expected block after for expression, found Semicolon
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected expression, found RParen
//     ╭─[ 0:29:10 ]
//     │
//  29 │    for ;;) {}
//     │          ┬  
//     │          ╰── Expected expression, found RParen
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected RParen, found For
//     ╭─[ 0:39:4 ]
//     │
//  39 │    for ;;i += x) {}
//     │    ─┬─  
//     │     ╰─── Expected RParen, found For
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected block after for expression, found For
//     ╭─[ 0:39:4 ]
//     │
//  39 │    for ;;i += x) {}
//     │    ─┬─  
//     │     ╰─── Expected block after for expression, found For
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected loop variable, found Semicolon
//     ╭─[ 0:39:8 ]
//     │
//  39 │    for ;;i += x) {}
//     │        ┬  
//     │        ╰── Expected loop variable, found Semicolon
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected In, found Semicolon
//     ╭─[ 0:39:8 ]
//     │
//  39 │    for ;;i += x) {}
//     │        ┬  
//     │        ╰── Expected In, found Semicolon
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected expression, found Semicolon
//     ╭─[ 0:39:8 ]
//     │
//  39 │    for ;;i += x) {}
//     │        ┬  
//     │        ╰── Expected expression, found Semicolon
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected block after for expression, found Semicolon
//     ╭─[ 0:39:9 ]
//     │
//  39 │    for ;;i += x) {}
//     │         ┬  
//     │         ╰── Expected block after for expression, found Semicolon
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected expression, found RParen
//     ╭─[ 0:39:16 ]
//     │
//  39 │    for ;;i += x) {}
//     │                ┬  
//     │                ╰── Expected expression, found RParen
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected loop variable, found Semicolon
//     ╭─[ 0:41:8 ]
//     │
//  41 │    for ;;i += x {}
//     │        ┬  
//     │        ╰── Expected loop variable, found Semicolon
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected In, found Semicolon
//     ╭─[ 0:41:8 ]
//     │
//  41 │    for ;;i += x {}
//     │        ┬  
//     │        ╰── Expected In, found Semicolon
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected expression, found Semicolon
//     ╭─[ 0:41:8 ]
//     │
//  41 │    for ;;i += x {}
//     │        ┬  
//     │        ╰── Expected expression, found Semicolon
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected block after for expression, found Semicolon
//     ╭─[ 0:41:9 ]
//     │
//  41 │    for ;;i += x {}
//     │         ┬  
//     │         ╰── Expected block after for expression, found Semicolon
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected RParen, found For
//     ╭─[ 0:49:4 ]
//     │
//  49 │    for ;;i = x) {}
//     │    ─┬─  
//     │     ╰─── Expected RParen, found For
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected block after for expression, found For
//     ╭─[ 0:49:4 ]
//     │
//  49 │    for ;;i = x) {}
//     │    ─┬─  
//     │     ╰─── Expected block after for expression, found For
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected loop variable, found Semicolon
//     ╭─[ 0:49:8 ]
//     │
//  49 │    for ;;i = x) {}
//     │        ┬  
//     │        ╰── Expected loop variable, found Semicolon
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected In, found Semicolon
//     ╭─[ 0:49:8 ]
//     │
//  49 │    for ;;i = x) {}
//     │        ┬  
//     │        ╰── Expected In, found Semicolon
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected expression, found Semicolon
//     ╭─[ 0:49:8 ]
//     │
//  49 │    for ;;i = x) {}
//     │        ┬  
//     │        ╰── Expected expression, found Semicolon
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected block after for expression, found Semicolon
//     ╭─[ 0:49:9 ]
//     │
//  49 │    for ;;i = x) {}
//     │         ┬  
//     │         ╰── Expected block after for expression, found Semicolon
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected expression, found RParen
//     ╭─[ 0:49:15 ]
//     │
//  49 │    for ;;i = x) {}
//     │               ┬  
//     │               ╰── Expected expression, found RParen
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected loop variable, found Semicolon
//     ╭─[ 0:51:8 ]
//     │
//  51 │    for ;;i = x {}
//     │        ┬  
//     │        ╰── Expected loop variable, found Semicolon
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected In, found Semicolon
//     ╭─[ 0:51:8 ]
//     │
//  51 │    for ;;i = x {}
//     │        ┬  
//     │        ╰── Expected In, found Semicolon
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected expression, found Semicolon
//     ╭─[ 0:51:8 ]
//     │
//  51 │    for ;;i = x {}
//     │        ┬  
//     │        ╰── Expected expression, found Semicolon
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected block after for expression, found Semicolon
//     ╭─[ 0:51:9 ]
//     │
//  51 │    for ;;i = x {}
//     │         ┬  
//     │         ╰── Expected block after for expression, found Semicolon
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected loop variable, found Let
//     ╭─[ 0:55:8 ]
//     │
//  55 │    for let a in as {}
//     │        ─┬─  
//     │         ╰─── Expected loop variable, found Let
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected In, found Let
//     ╭─[ 0:55:8 ]
//     │
//  55 │    for let a in as {}
//     │        ─┬─  
//     │         ╰─── Expected In, found Let
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected expression, found Let
//     ╭─[ 0:55:8 ]
//     │
//  55 │    for let a in as {}
//     │        ─┬─  
//     │         ╰─── Expected expression, found Let
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected block after for expression, found Word
//     ╭─[ 0:55:12 ]
//     │
//  55 │    for let a in as {}
//     │            ┬  
//     │            ╰── Expected block after for expression, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected expression, found In
//     ╭─[ 0:55:14 ]
//     │
//  55 │    for let a in as {}
//     │              ─┬  
//     │               ╰── Expected expression, found In
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected RParen, found For
//     ╭─[ 0:59:4 ]
//     │
//  59 │    for let a in as) {}
//     │    ─┬─  
//     │     ╰─── Expected RParen, found For
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected block after for expression, found For
//     ╭─[ 0:59:4 ]
//     │
//  59 │    for let a in as) {}
//     │    ─┬─  
//     │     ╰─── Expected block after for expression, found For
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected loop variable, found Let
//     ╭─[ 0:59:8 ]
//     │
//  59 │    for let a in as) {}
//     │        ─┬─  
//     │         ╰─── Expected loop variable, found Let
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected In, found Let
//     ╭─[ 0:59:8 ]
//     │
//  59 │    for let a in as) {}
//     │        ─┬─  
//     │         ╰─── Expected In, found Let
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected expression, found Let
//     ╭─[ 0:59:8 ]
//     │
//  59 │    for let a in as) {}
//     │        ─┬─  
//     │         ╰─── Expected expression, found Let
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected block after for expression, found Word
//     ╭─[ 0:59:12 ]
//     │
//  59 │    for let a in as) {}
//     │            ┬  
//     │            ╰── Expected block after for expression, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected expression, found In
//     ╭─[ 0:59:14 ]
//     │
//  59 │    for let a in as) {}
//     │              ─┬  
//     │               ╰── Expected expression, found In
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected expression, found RParen
//     ╭─[ 0:59:19 ]
//     │
//  59 │    for let a in as) {}
//     │                   ┬  
//     │                   ╰── Expected expression, found RParen
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected bool, found true
//    ╭─[ 0:5:17 ]
//    │
//  5 │ ╭─▶    if (true) { }
//    ┆ ┆   
//  7 │ ├─▶    if true {}
//    │ │                   
//    │ ╰─────────────────── Expected bool, found true
//    │     
//    │     Note: Error code: E0001
// ───╯
// Error: Expected bool, found true
//     ╭─[ 0:9:16 ]
//     │
//   9 │ ╭─▶    if true) { }
//     ┆ ┆   
//  11 │ ├─▶    if (true { }
//     │ │                     
//     │ ╰───────────────────── Expected bool, found true
//     │     
//     │     Note: Error code: E0001
// ────╯
// Error: Expected bool, found true
//     ╭─[ 0:15:10 ]
//     │
//  15 │    while true {}
//     │          ───┬───  
//     │             ╰───── Expected bool, found true
//     │ 
//     │ Note: Error code: E0001
// ────╯
// Error: Expected bool, found true
//     ╭─[ 0:17:11 ]
//     │
//  17 │    while (true {}
//     │           ───┬───  
//     │              ╰───── Expected bool, found true
//     │ 
//     │ Note: Error code: E0001
// ────╯
// Error: Unknown variable i
//     ╭─[ 0:35:11 ]
//     │
//  35 │    for (;;i += x) {}
//     │           ┬  
//     │           ╰── Unknown variable i
//     │ 
//     │ Note: Error code: E0003
// ────╯
// Error: Unknown variable x
//     ╭─[ 0:35:16 ]
//     │
//  35 │    for (;;i += x) {}
//     │                ┬  
//     │                ╰── Unknown variable x
//     │ 
//     │ Note: Error code: E0003
// ────╯
// Error: Unknown variable i
//     ╭─[ 0:37:11 ]
//     │
//  37 │    for (;;i += x {}
//     │           ┬  
//     │           ╰── Unknown variable i
//     │ 
//     │ Note: Error code: E0003
// ────╯
// Error: Unknown variable i
//     ╭─[ 0:39:10 ]
//     │
//  39 │    for ;;i += x) {}
//     │          ┬  
//     │          ╰── Unknown variable i
//     │ 
//     │ Note: Error code: E0003
// ────╯
// Error: Unknown variable x
//     ╭─[ 0:39:15 ]
//     │
//  39 │    for ;;i += x) {}
//     │               ┬  
//     │               ╰── Unknown variable x
//     │ 
//     │ Note: Error code: E0003
// ────╯
// Error: Unknown variable i
//     ╭─[ 0:41:10 ]
//     │
//  41 │    for ;;i += x {}
//     │          ┬  
//     │          ╰── Unknown variable i
//     │ 
//     │ Note: Error code: E0003
// ────╯
// Error: Unknown variable i
//     ╭─[ 0:45:11 ]
//     │
//  45 │    for (;;i = x) {}
//     │           ┬  
//     │           ╰── Unknown variable i
//     │ 
//     │ Note: Error code: E0003
// ────╯
// Error: Unknown variable x
//     ╭─[ 0:45:15 ]
//     │
//  45 │    for (;;i = x) {}
//     │               ┬  
//     │               ╰── Unknown variable x
//     │ 
//     │ Note: Error code: E0003
// ────╯
// Error: Unknown variable i
//     ╭─[ 0:47:11 ]
//     │
//  47 │    for (;;i = x {}
//     │           ┬  
//     │           ╰── Unknown variable i
//     │ 
//     │ Note: Error code: E0003
// ────╯
// Error: Unknown variable i
//     ╭─[ 0:49:10 ]
//     │
//  49 │    for ;;i = x) {}
//     │          ┬  
//     │          ╰── Unknown variable i
//     │ 
//     │ Note: Error code: E0003
// ────╯
// Error: Unknown variable x
//     ╭─[ 0:49:14 ]
//     │
//  49 │    for ;;i = x) {}
//     │              ┬  
//     │              ╰── Unknown variable x
//     │ 
//     │ Note: Error code: E0003
// ────╯
// Error: Unknown variable i
//     ╭─[ 0:51:10 ]
//     │
//  51 │    for ;;i = x {}
//     │          ┬  
//     │          ╰── Unknown variable i
//     │ 
//     │ Note: Error code: E0003
// ────╯
// Error: Unknown variable a
//     ╭─[ 0:55:12 ]
//     │
//  55 │    for let a in as {}
//     │            ┬  
//     │            ╰── Unknown variable a
//     │ 
//     │ Note: Error code: E0003
// ────╯
// Error: Type as has no field 'length'
//    ╭─[ 0:1:1 ]
//    │
//  1 │ function foo() -> int {
//    │ │ 
//    │ ╰─ Type as has no field 'length'
//    │ 
//    │ Note: Error code: E0007
// ───╯
// Error: Type as is not indexable
//    ╭─[ 0:1:1 ]
//    │
//  1 │ function foo() -> int {
//    │ │ 
//    │ ╰─ Type as is not indexable
//    │ 
//    │ Note: Error code: E0008
// ───╯
// Error: Unknown variable a
//     ╭─[ 0:59:12 ]
//     │
//  59 │    for let a in as) {}
//     │            ┬  
//     │            ╰── Unknown variable a
//     │ 
//     │ Note: Error code: E0003
// ────╯
// Error: Unknown variable as
//     ╭─[ 0:59:17 ]
//     │
//  59 │    for let a in as) {}
//     │                 ─┬  
//     │                  ╰── Unknown variable as
//     │ 
//     │ Note: Error code: E0003
// ────╯
// Error: Unknown variable as
//    ╭─[ 0:1:1 ]
//    │
//  1 │ function foo() -> int {
//    │ │ 
//    │ ╰─ Unknown variable as
//    │ 
//    │ Note: Error code: E0003
// ───╯
// Error: Expected int, found void
//    ╭─[ 0:1:1 ]
//    │
//  1 │ function foo() -> int {
//    │ │ 
//    │ ╰─ Expected int, found void
//    │ 
//    │ Note: Error code: E0001
// ───╯
