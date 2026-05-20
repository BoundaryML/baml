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
// error: Expected block after if condition, found ')'
//    ╭─[ parens.baml:9:11 ]
//    │
//  9 │    if true) { }
//    │           ┬  
//    │           ╰── Expected block after if condition, found ')'
//    │ 
//    │ Note: Error code: E0010
// ───╯
// error: Expected expression, found ')'
//    ╭─[ parens.baml:9:11 ]
//    │
//  9 │    if true) { }
//    │           ┬  
//    │           ╰── Expected expression, found ')'
//    │ 
//    │ Note: Error code: E0010
// ───╯
// error: Expected ')', found '{'
//     ╭─[ parens.baml:11:13 ]
//     │
//  11 │    if (true { }
//     │             ┬  
//     │             ╰── Expected ')', found '{'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// error: Expected ')', found '{'
//     ╭─[ parens.baml:17:16 ]
//     │
//  17 │    while (true {}
//     │                ┬  
//     │                ╰── Expected ')', found '{'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// error: Expected block after while condition, found ')'
//     ╭─[ parens.baml:19:14 ]
//     │
//  19 │    while true) {}
//     │              ┬  
//     │              ╰── Expected block after while condition, found ')'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// error: Expected expression, found ')'
//     ╭─[ parens.baml:19:14 ]
//     │
//  19 │    while true) {}
//     │              ┬  
//     │              ╰── Expected expression, found ')'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// error: Expected 'let', found ';'
//     ╭─[ parens.baml:25:8 ]
//     │
//  25 │    for ;; { }
//     │        ┬  
//     │        ╰── Expected 'let', found ';'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// error: Expected pattern, found ';'
//     ╭─[ parens.baml:25:8 ]
//     │
//  25 │    for ;; { }
//     │        ┬  
//     │        ╰── Expected pattern, found ';'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// error: Expected in, found ';'
//     ╭─[ parens.baml:25:9 ]
//     │
//  25 │    for ;; { }
//     │         ┬  
//     │         ╰── Expected in, found ';'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// error: Expected expression, found ';'
//     ╭─[ parens.baml:25:9 ]
//     │
//  25 │    for ;; { }
//     │         ┬  
//     │         ╰── Expected expression, found ';'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// error: Expected ')', found for
//     ╭─[ parens.baml:29:4 ]
//     │
//  29 │    for ;;) {}
//     │    ─┬─  
//     │     ╰─── Expected ')', found for
//     │ 
//     │ Note: Error code: E0010
// ────╯
// error: Expected block after for expression, found for
//     ╭─[ parens.baml:29:4 ]
//     │
//  29 │    for ;;) {}
//     │    ─┬─  
//     │     ╰─── Expected block after for expression, found for
//     │ 
//     │ Note: Error code: E0010
// ────╯
// error: Expected 'let', found ';'
//     ╭─[ parens.baml:29:8 ]
//     │
//  29 │    for ;;) {}
//     │        ┬  
//     │        ╰── Expected 'let', found ';'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// error: Expected pattern, found ';'
//     ╭─[ parens.baml:29:8 ]
//     │
//  29 │    for ;;) {}
//     │        ┬  
//     │        ╰── Expected pattern, found ';'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// error: Expected in, found ';'
//     ╭─[ parens.baml:29:9 ]
//     │
//  29 │    for ;;) {}
//     │         ┬  
//     │         ╰── Expected in, found ';'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// error: Expected expression, found ';'
//     ╭─[ parens.baml:29:9 ]
//     │
//  29 │    for ;;) {}
//     │         ┬  
//     │         ╰── Expected expression, found ';'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// error: Expected block after for expression, found ')'
//     ╭─[ parens.baml:29:10 ]
//     │
//  29 │    for ;;) {}
//     │          ┬  
//     │          ╰── Expected block after for expression, found ')'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// error: Expected expression, found ')'
//     ╭─[ parens.baml:29:10 ]
//     │
//  29 │    for ;;) {}
//     │          ┬  
//     │          ╰── Expected expression, found ')'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// error: Expected ')', found for
//     ╭─[ parens.baml:39:4 ]
//     │
//  39 │    for ;;i += x) {}
//     │    ─┬─  
//     │     ╰─── Expected ')', found for
//     │ 
//     │ Note: Error code: E0010
// ────╯
// error: Expected block after for expression, found for
//     ╭─[ parens.baml:39:4 ]
//     │
//  39 │    for ;;i += x) {}
//     │    ─┬─  
//     │     ╰─── Expected block after for expression, found for
//     │ 
//     │ Note: Error code: E0010
// ────╯
// error: Expected 'let', found ';'
//     ╭─[ parens.baml:39:8 ]
//     │
//  39 │    for ;;i += x) {}
//     │        ┬  
//     │        ╰── Expected 'let', found ';'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// error: Expected pattern, found ';'
//     ╭─[ parens.baml:39:8 ]
//     │
//  39 │    for ;;i += x) {}
//     │        ┬  
//     │        ╰── Expected pattern, found ';'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// error: Expected in, found ';'
//     ╭─[ parens.baml:39:9 ]
//     │
//  39 │    for ;;i += x) {}
//     │         ┬  
//     │         ╰── Expected in, found ';'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// error: Expected expression, found ';'
//     ╭─[ parens.baml:39:9 ]
//     │
//  39 │    for ;;i += x) {}
//     │         ┬  
//     │         ╰── Expected expression, found ';'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// error: Expected block after for expression, found identifier
//     ╭─[ parens.baml:39:10 ]
//     │
//  39 │    for ;;i += x) {}
//     │          ┬  
//     │          ╰── Expected block after for expression, found identifier
//     │ 
//     │ Note: Error code: E0010
// ────╯
// error: Expected expression, found ')'
//     ╭─[ parens.baml:39:16 ]
//     │
//  39 │    for ;;i += x) {}
//     │                ┬  
//     │                ╰── Expected expression, found ')'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// error: Expected 'let', found ';'
//     ╭─[ parens.baml:41:8 ]
//     │
//  41 │    for ;;i += x {}
//     │        ┬  
//     │        ╰── Expected 'let', found ';'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// error: Expected pattern, found ';'
//     ╭─[ parens.baml:41:8 ]
//     │
//  41 │    for ;;i += x {}
//     │        ┬  
//     │        ╰── Expected pattern, found ';'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// error: Expected in, found ';'
//     ╭─[ parens.baml:41:9 ]
//     │
//  41 │    for ;;i += x {}
//     │         ┬  
//     │         ╰── Expected in, found ';'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// error: Expected expression, found ';'
//     ╭─[ parens.baml:41:9 ]
//     │
//  41 │    for ;;i += x {}
//     │         ┬  
//     │         ╰── Expected expression, found ';'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// error: Expected block after for expression, found identifier
//     ╭─[ parens.baml:41:10 ]
//     │
//  41 │    for ;;i += x {}
//     │          ┬  
//     │          ╰── Expected block after for expression, found identifier
//     │ 
//     │ Note: Error code: E0010
// ────╯
// error: Expected ')', found for
//     ╭─[ parens.baml:49:4 ]
//     │
//  49 │    for ;;i = x) {}
//     │    ─┬─  
//     │     ╰─── Expected ')', found for
//     │ 
//     │ Note: Error code: E0010
// ────╯
// error: Expected block after for expression, found for
//     ╭─[ parens.baml:49:4 ]
//     │
//  49 │    for ;;i = x) {}
//     │    ─┬─  
//     │     ╰─── Expected block after for expression, found for
//     │ 
//     │ Note: Error code: E0010
// ────╯
// error: Expected 'let', found ';'
//     ╭─[ parens.baml:49:8 ]
//     │
//  49 │    for ;;i = x) {}
//     │        ┬  
//     │        ╰── Expected 'let', found ';'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// error: Expected pattern, found ';'
//     ╭─[ parens.baml:49:8 ]
//     │
//  49 │    for ;;i = x) {}
//     │        ┬  
//     │        ╰── Expected pattern, found ';'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// error: Expected in, found ';'
//     ╭─[ parens.baml:49:9 ]
//     │
//  49 │    for ;;i = x) {}
//     │         ┬  
//     │         ╰── Expected in, found ';'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// error: Expected expression, found ';'
//     ╭─[ parens.baml:49:9 ]
//     │
//  49 │    for ;;i = x) {}
//     │         ┬  
//     │         ╰── Expected expression, found ';'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// error: Expected block after for expression, found identifier
//     ╭─[ parens.baml:49:10 ]
//     │
//  49 │    for ;;i = x) {}
//     │          ┬  
//     │          ╰── Expected block after for expression, found identifier
//     │ 
//     │ Note: Error code: E0010
// ────╯
// error: Expected expression, found ')'
//     ╭─[ parens.baml:49:15 ]
//     │
//  49 │    for ;;i = x) {}
//     │               ┬  
//     │               ╰── Expected expression, found ')'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// error: Expected 'let', found ';'
//     ╭─[ parens.baml:51:8 ]
//     │
//  51 │    for ;;i = x {}
//     │        ┬  
//     │        ╰── Expected 'let', found ';'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// error: Expected pattern, found ';'
//     ╭─[ parens.baml:51:8 ]
//     │
//  51 │    for ;;i = x {}
//     │        ┬  
//     │        ╰── Expected pattern, found ';'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// error: Expected in, found ';'
//     ╭─[ parens.baml:51:9 ]
//     │
//  51 │    for ;;i = x {}
//     │         ┬  
//     │         ╰── Expected in, found ';'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// error: Expected expression, found ';'
//     ╭─[ parens.baml:51:9 ]
//     │
//  51 │    for ;;i = x {}
//     │         ┬  
//     │         ╰── Expected expression, found ';'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// error: Expected block after for expression, found identifier
//     ╭─[ parens.baml:51:10 ]
//     │
//  51 │    for ;;i = x {}
//     │          ┬  
//     │          ╰── Expected block after for expression, found identifier
//     │ 
//     │ Note: Error code: E0010
// ────╯
// error: Expected block after for expression, found for
//     ╭─[ parens.baml:57:4 ]
//     │
//  57 │    for (let a in as {}
//     │    ─┬─  
//     │     ╰─── Expected block after for expression, found for
//     │ 
//     │ Note: Error code: E0010
// ────╯
// error: Expected ')', found for
//     ╭─[ parens.baml:59:4 ]
//     │
//  59 │    for let a in as) {}
//     │    ─┬─  
//     │     ╰─── Expected ')', found for
//     │ 
//     │ Note: Error code: E0010
// ────╯
// error: Expected block after for expression, found for
//     ╭─[ parens.baml:59:4 ]
//     │
//  59 │    for let a in as) {}
//     │    ─┬─  
//     │     ╰─── Expected block after for expression, found for
//     │ 
//     │ Note: Error code: E0010
// ────╯
// error: Expected block after for expression, found ')'
//     ╭─[ parens.baml:59:19 ]
//     │
//  59 │    for let a in as) {}
//     │                   ┬  
//     │                   ╰── Expected block after for expression, found ')'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// error: Expected expression, found ')'
//     ╭─[ parens.baml:59:19 ]
//     │
//  59 │    for let a in as) {}
//     │                   ┬  
//     │                   ╰── Expected expression, found ')'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// error: unresolved name: i
//     ╭─[ parens.baml:39:10 ]
//     │
//  39 │    for ;;i += x) {}
//     │          ┬  
//     │          ╰── unresolved name: i
//     │ 
//     │ Note: Error code: E0003
// ────╯
// error: unresolved name: x
//     ╭─[ parens.baml:39:15 ]
//     │
//  39 │    for ;;i += x) {}
//     │               ┬  
//     │               ╰── unresolved name: x
//     │ 
//     │ Note: Error code: E0003
// ────╯
// error: unresolved name: i
//     ╭─[ parens.baml:41:10 ]
//     │
//  41 │    for ;;i += x {}
//     │          ┬  
//     │          ╰── unresolved name: i
//     │ 
//     │ Note: Error code: E0003
// ────╯
// error: unresolved name: x
//     ╭─[ parens.baml:49:14 ]
//     │
//  49 │    for ;;i = x) {}
//     │              ┬  
//     │              ╰── unresolved name: x
//     │ 
//     │ Note: Error code: E0003
// ────╯
// error: unresolved name: i
//     ╭─[ parens.baml:49:10 ]
//     │
//  49 │    for ;;i = x) {}
//     │          ┬  
//     │          ╰── unresolved name: i
//     │ 
//     │ Note: Error code: E0003
// ────╯
// error: unresolved name: x
//     ╭─[ parens.baml:49:14 ]
//     │
//  49 │    for ;;i = x) {}
//     │              ┬  
//     │              ╰── unresolved name: x
//     │ 
//     │ Note: Error code: E0003
// ────╯
// error: unresolved name: i
//     ╭─[ parens.baml:51:10 ]
//     │
//  51 │    for ;;i = x {}
//     │          ┬  
//     │          ╰── unresolved name: i
//     │ 
//     │ Note: Error code: E0003
// ────╯
// error: cannot iterate over type `unknown`
//     ╭─[ parens.baml:55:17 ]
//     │
//  55 │    for let a in as {}
//     │                 ──┬──  
//     │                   ╰──── cannot iterate over type `unknown`
//     │ 
//     │ Note: Error code: E0006
// ────╯
// error: cannot iterate over type `unknown`
//     ╭─[ parens.baml:57:18 ]
//     │
//  57 │    for (let a in as {}
//     │                  ──┬──  
//     │                    ╰──── cannot iterate over type `unknown`
//     │ 
//     │ Note: Error code: E0006
// ────╯
// error: unresolved name: as
//     ╭─[ parens.baml:59:17 ]
//     │
//  59 │    for let a in as) {}
//     │                 ─┬  
//     │                  ╰── unresolved name: as
//     │ 
//     │ Note: Error code: E0003
// ────╯
// error: cannot iterate over type `unknown`
//     ╭─[ parens.baml:59:17 ]
//     │
//  59 │    for let a in as) {}
//     │                 ─┬  
//     │                  ╰── cannot iterate over type `unknown`
//     │ 
//     │ Note: Error code: E0006
// ────╯
// error: unresolved name: as
//     ╭─[ parens.baml:61:18 ]
//     │
//  61 │    for (let a in as) {}
//     │                  ─┬  
//     │                   ╰── unresolved name: as
//     │ 
//     │ Note: Error code: E0003
// ────╯
// error: cannot iterate over type `unknown`
//     ╭─[ parens.baml:61:18 ]
//     │
//  61 │    for (let a in as) {}
//     │                  ─┬  
//     │                   ╰── cannot iterate over type `unknown`
//     │ 
//     │ Note: Error code: E0006
// ────╯
// error: missing return: expected `int`
//     ╭─[ parens.baml:1:22 ]
//     │
//   1 │ ╭─▶ function foo() -> int {
//     ┆ ┆   
//  63 │ ├─▶ }
//     │ │       
//     │ ╰─────── missing return: expected `int`
//     │     
//     │     Note: Error code: E0029
// ────╯
