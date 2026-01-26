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
// Error: Expected block after if condition, found if
//    ╭─[ parens.baml:9:4 ]
//    │
//  9 │    if true) { }
//    │    ─┬  
//    │     ╰── Expected block after if condition, found if
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected block after if condition, found ')'
//    ╭─[ parens.baml:9:11 ]
//    │
//  9 │    if true) { }
//    │           ┬  
//    │           ╰── Expected block after if condition, found ')'
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected expression, found ')'
//    ╭─[ parens.baml:9:11 ]
//    │
//  9 │    if true) { }
//    │           ┬  
//    │           ╰── Expected expression, found ')'
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected ')', found while
//     ╭─[ parens.baml:15:4 ]
//     │
//  15 │    while true {}
//     │    ──┬──  
//     │      ╰──── Expected ')', found while
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected block after if condition, found while
//     ╭─[ parens.baml:15:4 ]
//     │
//  15 │    while true {}
//     │    ──┬──  
//     │      ╰──── Expected block after if condition, found while
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected block after while condition, found while
//     ╭─[ parens.baml:17:4 ]
//     │
//  17 │    while (true {}
//     │    ──┬──  
//     │      ╰──── Expected block after while condition, found while
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected ')', found while
//     ╭─[ parens.baml:19:4 ]
//     │
//  19 │    while true) {}
//     │    ──┬──  
//     │      ╰──── Expected ')', found while
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected block after while condition, found while
//     ╭─[ parens.baml:19:4 ]
//     │
//  19 │    while true) {}
//     │    ──┬──  
//     │      ╰──── Expected block after while condition, found while
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected block after while condition, found ')'
//     ╭─[ parens.baml:19:14 ]
//     │
//  19 │    while true) {}
//     │              ┬  
//     │              ╰── Expected block after while condition, found ')'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected expression, found ')'
//     ╭─[ parens.baml:19:14 ]
//     │
//  19 │    while true) {}
//     │              ┬  
//     │              ╰── Expected expression, found ')'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected loop variable, found ';'
//     ╭─[ parens.baml:25:8 ]
//     │
//  25 │    for ;; { }
//     │        ┬  
//     │        ╰── Expected loop variable, found ';'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected in, found ';'
//     ╭─[ parens.baml:25:8 ]
//     │
//  25 │    for ;; { }
//     │        ┬  
//     │        ╰── Expected in, found ';'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected expression, found ';'
//     ╭─[ parens.baml:25:8 ]
//     │
//  25 │    for ;; { }
//     │        ┬  
//     │        ╰── Expected expression, found ';'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected block after for expression, found ';'
//     ╭─[ parens.baml:25:9 ]
//     │
//  25 │    for ;; { }
//     │         ┬  
//     │         ╰── Expected block after for expression, found ';'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected ')', found for
//     ╭─[ parens.baml:29:4 ]
//     │
//  29 │    for ;;) {}
//     │    ─┬─  
//     │     ╰─── Expected ')', found for
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected block after for expression, found for
//     ╭─[ parens.baml:29:4 ]
//     │
//  29 │    for ;;) {}
//     │    ─┬─  
//     │     ╰─── Expected block after for expression, found for
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected loop variable, found ';'
//     ╭─[ parens.baml:29:8 ]
//     │
//  29 │    for ;;) {}
//     │        ┬  
//     │        ╰── Expected loop variable, found ';'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected in, found ';'
//     ╭─[ parens.baml:29:8 ]
//     │
//  29 │    for ;;) {}
//     │        ┬  
//     │        ╰── Expected in, found ';'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected expression, found ';'
//     ╭─[ parens.baml:29:8 ]
//     │
//  29 │    for ;;) {}
//     │        ┬  
//     │        ╰── Expected expression, found ';'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected block after for expression, found ';'
//     ╭─[ parens.baml:29:9 ]
//     │
//  29 │    for ;;) {}
//     │         ┬  
//     │         ╰── Expected block after for expression, found ';'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected expression, found ')'
//     ╭─[ parens.baml:29:10 ]
//     │
//  29 │    for ;;) {}
//     │          ┬  
//     │          ╰── Expected expression, found ')'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected ')', found for
//     ╭─[ parens.baml:39:4 ]
//     │
//  39 │    for ;;i += x) {}
//     │    ─┬─  
//     │     ╰─── Expected ')', found for
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected block after for expression, found for
//     ╭─[ parens.baml:39:4 ]
//     │
//  39 │    for ;;i += x) {}
//     │    ─┬─  
//     │     ╰─── Expected block after for expression, found for
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected loop variable, found ';'
//     ╭─[ parens.baml:39:8 ]
//     │
//  39 │    for ;;i += x) {}
//     │        ┬  
//     │        ╰── Expected loop variable, found ';'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected in, found ';'
//     ╭─[ parens.baml:39:8 ]
//     │
//  39 │    for ;;i += x) {}
//     │        ┬  
//     │        ╰── Expected in, found ';'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected expression, found ';'
//     ╭─[ parens.baml:39:8 ]
//     │
//  39 │    for ;;i += x) {}
//     │        ┬  
//     │        ╰── Expected expression, found ';'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected block after for expression, found ';'
//     ╭─[ parens.baml:39:9 ]
//     │
//  39 │    for ;;i += x) {}
//     │         ┬  
//     │         ╰── Expected block after for expression, found ';'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected expression, found ')'
//     ╭─[ parens.baml:39:16 ]
//     │
//  39 │    for ;;i += x) {}
//     │                ┬  
//     │                ╰── Expected expression, found ')'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected loop variable, found ';'
//     ╭─[ parens.baml:41:8 ]
//     │
//  41 │    for ;;i += x {}
//     │        ┬  
//     │        ╰── Expected loop variable, found ';'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected in, found ';'
//     ╭─[ parens.baml:41:8 ]
//     │
//  41 │    for ;;i += x {}
//     │        ┬  
//     │        ╰── Expected in, found ';'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected expression, found ';'
//     ╭─[ parens.baml:41:8 ]
//     │
//  41 │    for ;;i += x {}
//     │        ┬  
//     │        ╰── Expected expression, found ';'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected block after for expression, found ';'
//     ╭─[ parens.baml:41:9 ]
//     │
//  41 │    for ;;i += x {}
//     │         ┬  
//     │         ╰── Expected block after for expression, found ';'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected ')', found for
//     ╭─[ parens.baml:49:4 ]
//     │
//  49 │    for ;;i = x) {}
//     │    ─┬─  
//     │     ╰─── Expected ')', found for
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected block after for expression, found for
//     ╭─[ parens.baml:49:4 ]
//     │
//  49 │    for ;;i = x) {}
//     │    ─┬─  
//     │     ╰─── Expected block after for expression, found for
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected loop variable, found ';'
//     ╭─[ parens.baml:49:8 ]
//     │
//  49 │    for ;;i = x) {}
//     │        ┬  
//     │        ╰── Expected loop variable, found ';'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected in, found ';'
//     ╭─[ parens.baml:49:8 ]
//     │
//  49 │    for ;;i = x) {}
//     │        ┬  
//     │        ╰── Expected in, found ';'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected expression, found ';'
//     ╭─[ parens.baml:49:8 ]
//     │
//  49 │    for ;;i = x) {}
//     │        ┬  
//     │        ╰── Expected expression, found ';'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected block after for expression, found ';'
//     ╭─[ parens.baml:49:9 ]
//     │
//  49 │    for ;;i = x) {}
//     │         ┬  
//     │         ╰── Expected block after for expression, found ';'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected expression, found ')'
//     ╭─[ parens.baml:49:15 ]
//     │
//  49 │    for ;;i = x) {}
//     │               ┬  
//     │               ╰── Expected expression, found ')'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected loop variable, found ';'
//     ╭─[ parens.baml:51:8 ]
//     │
//  51 │    for ;;i = x {}
//     │        ┬  
//     │        ╰── Expected loop variable, found ';'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected in, found ';'
//     ╭─[ parens.baml:51:8 ]
//     │
//  51 │    for ;;i = x {}
//     │        ┬  
//     │        ╰── Expected in, found ';'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected expression, found ';'
//     ╭─[ parens.baml:51:8 ]
//     │
//  51 │    for ;;i = x {}
//     │        ┬  
//     │        ╰── Expected expression, found ';'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected block after for expression, found ';'
//     ╭─[ parens.baml:51:9 ]
//     │
//  51 │    for ;;i = x {}
//     │         ┬  
//     │         ╰── Expected block after for expression, found ';'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected loop variable, found let
//     ╭─[ parens.baml:55:8 ]
//     │
//  55 │    for let a in as {}
//     │        ─┬─  
//     │         ╰─── Expected loop variable, found let
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected in, found let
//     ╭─[ parens.baml:55:8 ]
//     │
//  55 │    for let a in as {}
//     │        ─┬─  
//     │         ╰─── Expected in, found let
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected expression, found let
//     ╭─[ parens.baml:55:8 ]
//     │
//  55 │    for let a in as {}
//     │        ─┬─  
//     │         ╰─── Expected expression, found let
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected block after for expression, found identifier
//     ╭─[ parens.baml:55:12 ]
//     │
//  55 │    for let a in as {}
//     │            ┬  
//     │            ╰── Expected block after for expression, found identifier
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected expression, found in
//     ╭─[ parens.baml:55:14 ]
//     │
//  55 │    for let a in as {}
//     │              ─┬  
//     │               ╰── Expected expression, found in
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected ')', found for
//     ╭─[ parens.baml:59:4 ]
//     │
//  59 │    for let a in as) {}
//     │    ─┬─  
//     │     ╰─── Expected ')', found for
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected block after for expression, found for
//     ╭─[ parens.baml:59:4 ]
//     │
//  59 │    for let a in as) {}
//     │    ─┬─  
//     │     ╰─── Expected block after for expression, found for
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected loop variable, found let
//     ╭─[ parens.baml:59:8 ]
//     │
//  59 │    for let a in as) {}
//     │        ─┬─  
//     │         ╰─── Expected loop variable, found let
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected in, found let
//     ╭─[ parens.baml:59:8 ]
//     │
//  59 │    for let a in as) {}
//     │        ─┬─  
//     │         ╰─── Expected in, found let
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected expression, found let
//     ╭─[ parens.baml:59:8 ]
//     │
//  59 │    for let a in as) {}
//     │        ─┬─  
//     │         ╰─── Expected expression, found let
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected block after for expression, found identifier
//     ╭─[ parens.baml:59:12 ]
//     │
//  59 │    for let a in as) {}
//     │            ┬  
//     │            ╰── Expected block after for expression, found identifier
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected expression, found in
//     ╭─[ parens.baml:59:14 ]
//     │
//  59 │    for let a in as) {}
//     │              ─┬  
//     │               ╰── Expected expression, found in
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected expression, found ')'
//     ╭─[ parens.baml:59:19 ]
//     │
//  59 │    for let a in as) {}
//     │                   ┬  
//     │                   ╰── Expected expression, found ')'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Condition in `if` statement must be wrapped in parentheses.
//    ╭─[ parens.baml:7:7 ]
//    │
//  7 │    if true {}
//    │       ───┬───  
//    │          ╰───── Condition in `if` statement must be wrapped in parentheses.
//    │ 
//    │ Note: Error code: E0030
// ───╯
// Error: Condition in `while` statement must be wrapped in parentheses.
//     ╭─[ parens.baml:15:10 ]
//     │
//  15 │    while true {}
//     │          ───┬───  
//     │             ╰───── Condition in `while` statement must be wrapped in parentheses.
//     │ 
//     │ Note: Error code: E0030
// ────╯
// Error: Unknown variable `i`
//     ╭─[ parens.baml:35:11 ]
//     │
//  35 │    for (;;i += x) {}
//     │           ┬  
//     │           ╰── Unknown variable `i`
//     │ 
//     │ Note: Error code: E0003
// ────╯
// Error: Unknown variable `x`
//     ╭─[ parens.baml:35:16 ]
//     │
//  35 │    for (;;i += x) {}
//     │                ┬  
//     │                ╰── Unknown variable `x`
//     │ 
//     │ Note: Error code: E0003
// ────╯
// Error: Unknown variable `i`
//     ╭─[ parens.baml:37:11 ]
//     │
//  37 │    for (;;i += x {}
//     │           ┬  
//     │           ╰── Unknown variable `i`
//     │ 
//     │ Note: Error code: E0003
// ────╯
// Error: Unknown variable `i`
//     ╭─[ parens.baml:39:10 ]
//     │
//  39 │    for ;;i += x) {}
//     │          ┬  
//     │          ╰── Unknown variable `i`
//     │ 
//     │ Note: Error code: E0003
// ────╯
// Error: Unknown variable `x`
//     ╭─[ parens.baml:39:15 ]
//     │
//  39 │    for ;;i += x) {}
//     │               ┬  
//     │               ╰── Unknown variable `x`
//     │ 
//     │ Note: Error code: E0003
// ────╯
// Error: Unknown variable `i`
//     ╭─[ parens.baml:41:10 ]
//     │
//  41 │    for ;;i += x {}
//     │          ┬  
//     │          ╰── Unknown variable `i`
//     │ 
//     │ Note: Error code: E0003
// ────╯
// Error: Unknown variable `i`
//     ╭─[ parens.baml:45:11 ]
//     │
//  45 │    for (;;i = x) {}
//     │           ┬  
//     │           ╰── Unknown variable `i`
//     │ 
//     │ Note: Error code: E0003
// ────╯
// Error: Unknown variable `x`
//     ╭─[ parens.baml:45:15 ]
//     │
//  45 │    for (;;i = x) {}
//     │               ┬  
//     │               ╰── Unknown variable `x`
//     │ 
//     │ Note: Error code: E0003
// ────╯
// Error: Unknown variable `i`
//     ╭─[ parens.baml:47:11 ]
//     │
//  47 │    for (;;i = x {}
//     │           ┬  
//     │           ╰── Unknown variable `i`
//     │ 
//     │ Note: Error code: E0003
// ────╯
// Error: Unknown variable `i`
//     ╭─[ parens.baml:49:10 ]
//     │
//  49 │    for ;;i = x) {}
//     │          ┬  
//     │          ╰── Unknown variable `i`
//     │ 
//     │ Note: Error code: E0003
// ────╯
// Error: Unknown variable `x`
//     ╭─[ parens.baml:49:14 ]
//     │
//  49 │    for ;;i = x) {}
//     │              ┬  
//     │              ╰── Unknown variable `x`
//     │ 
//     │ Note: Error code: E0003
// ────╯
// Error: Unknown variable `i`
//     ╭─[ parens.baml:51:10 ]
//     │
//  51 │    for ;;i = x {}
//     │          ┬  
//     │          ╰── Unknown variable `i`
//     │ 
//     │ Note: Error code: E0003
// ────╯
// Error: Unknown variable `a`
//     ╭─[ parens.baml:55:12 ]
//     │
//  55 │    for let a in as {}
//     │            ┬  
//     │            ╰── Unknown variable `a`
//     │ 
//     │ Note: Error code: E0003
// ────╯
// Error: Type `as` has no field `length`
//    ╭─[ parens.baml:1:1 ]
//    │
//  1 │ function foo() -> int {
//    │ │ 
//    │ ╰─ Type `as` has no field `length`
//    │ 
//    │ Note: Error code: E0007
// ───╯
// Error: Type `as` is not indexable
//    ╭─[ parens.baml:1:1 ]
//    │
//  1 │ function foo() -> int {
//    │ │ 
//    │ ╰─ Type `as` is not indexable
//    │ 
//    │ Note: Error code: E0008
// ───╯
// Error: Unknown variable `a`
//     ╭─[ parens.baml:59:12 ]
//     │
//  59 │    for let a in as) {}
//     │            ┬  
//     │            ╰── Unknown variable `a`
//     │ 
//     │ Note: Error code: E0003
// ────╯
// Error: Unknown variable `as`
//     ╭─[ parens.baml:59:17 ]
//     │
//  59 │    for let a in as) {}
//     │                 ─┬  
//     │                  ╰── Unknown variable `as`
//     │ 
//     │ Note: Error code: E0003
// ────╯
// Error: Unknown variable `as`
//    ╭─[ parens.baml:1:1 ]
//    │
//  1 │ function foo() -> int {
//    │ │ 
//    │ ╰─ Unknown variable `as`
//    │ 
//    │ Note: Error code: E0003
// ───╯
// Error: Missing return expression. Function expects `int` but body has no final expression.
//     ╭─[ parens.baml:1:22 ]
//     │
//   1 │ ╭─▶ function foo() -> int {
//     ┆ ┆   
//  63 │ ├─▶ }
//     │ │       
//     │ ╰─────── Missing return expression. Function expects `int` but body has no final expression.
//     │     
//     │     Note: Error code: E0029
// ────╯
