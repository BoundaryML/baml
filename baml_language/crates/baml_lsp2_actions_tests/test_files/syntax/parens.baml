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
// E0010
//
//   × unexpected token
//    ╭─[parens.baml:9:11]
//  9 │    if true) { }
//    ·           ┬
//    ·           ╰── expected `block after if condition`, found `')'`
//    ╰────
// E0010
//
//   × unexpected token
//     ╭─[parens.baml:11:13]
//  11 │    if (true { }
//     ·             ┬
//     ·             ╰── expected `')'`, found `'{'`
//     ╰────
// E0010
//
//   × unexpected token
//     ╭─[parens.baml:17:16]
//  17 │    while (true {}
//     ·                ┬
//     ·                ╰── expected `')'`, found `'{'`
//     ╰────
// E0010
//
//   × unexpected token
//     ╭─[parens.baml:19:14]
//  19 │    while true) {}
//     ·              ┬
//     ·              ╰── expected `block after while condition`, found `')'`
//     ╰────
// E0010
//
//   × unexpected token
//     ╭─[parens.baml:25:8]
//  25 │    for ;; { }
//     ·        ┬
//     ·        ╰── expected `'('`, found `';'`
//     ╰────
// E0010
//
//   × unexpected token
//     ╭─[parens.baml:27:12]
//  27 │    for (;; {}
//     ·            ┬
//     ·            ╰── expected `')'`, found `'{'`
//     ╰────
// E0010
//
//   × unexpected token
//     ╭─[parens.baml:29:8]
//  29 │    for ;;) {}
//     ·        ┬
//     ·        ╰── expected `'('`, found `';'`
//     ╰────
// E0010
//
//   × unexpected token
//     ╭─[parens.baml:39:4]
//  39 │    for ;;i += x) {}
//     ·    ─┬─
//     ·     ╰── expected `')'`, found `for`
//     ╰────
// E0010
//
//   × unexpected token
//     ╭─[parens.baml:39:8]
//  39 │    for ;;i += x) {}
//     ·        ┬
//     ·        ╰── expected `'('`, found `';'`
//     ╰────
// E0010
//
//   × unexpected token
//     ╭─[parens.baml:41:8]
//  41 │    for ;;i += x {}
//     ·        ┬
//     ·        ╰── expected `'('`, found `';'`
//     ╰────
// E0010
//
//   × unexpected token
//     ╭─[parens.baml:49:4]
//  49 │    for ;;i = x) {}
//     ·    ─┬─
//     ·     ╰── expected `')'`, found `for`
//     ╰────
// E0010
//
//   × unexpected token
//     ╭─[parens.baml:49:8]
//  49 │    for ;;i = x) {}
//     ·        ┬
//     ·        ╰── expected `'('`, found `';'`
//     ╰────
// E0010
//
//   × unexpected token
//     ╭─[parens.baml:51:8]
//  51 │    for ;;i = x {}
//     ·        ┬
//     ·        ╰── expected `'('`, found `';'`
//     ╰────
// E0010
//
//   × unexpected token
//     ╭─[parens.baml:57:21]
//  57 │    for (let a in as {}
//     ·                     ┬
//     ·                     ╰── expected `')'`, found `'{'`
//     ╰────
// E0010
//
//   × unexpected token
//     ╭─[parens.baml:59:19]
//  59 │    for let a in as) {}
//     ·                   ┬
//     ·                   ╰── expected `block after for expression`, found `')'`
//     ╰────
// E0010
//
//   × assignment is not allowed in expression position; assignment is a statement
//     ╭─[parens.baml:35:11]
//  35 │    for (;;i += x) {}
//     ·           ───┬──
//     ·              ╰── assignment not allowed here
//     ╰────
// E0010
//
//   × assignment is not allowed in expression position; assignment is a statement
//     ╭─[parens.baml:37:11]
//  37 │    for (;;i += x {}
//     ·           ────┬────
//     ·               ╰── assignment not allowed here
//     ╰────
// E0010
//
//   × assignment is not allowed in expression position; assignment is a statement
//     ╭─[parens.baml:39:10]
//  39 │    for ;;i += x) {}
//     ·          ───┬──
//     ·             ╰── assignment not allowed here
//     ╰────
// E0010
//
//   × assignment is not allowed in expression position; assignment is a statement
//     ╭─[parens.baml:41:10]
//  41 │    for ;;i += x {}
//     ·          ───┬──
//     ·             ╰── assignment not allowed here
//     ╰────
// E0010
//
//   × assignment is not allowed in expression position; assignment is a statement
//     ╭─[parens.baml:45:11]
//  45 │    for (;;i = x) {}
//     ·           ──┬──
//     ·             ╰── assignment not allowed here
//     ╰────
// E0010
//
//   × assignment is not allowed in expression position; assignment is a statement
//     ╭─[parens.baml:47:11]
//  47 │    for (;;i = x {}
//     ·           ────┬───
//     ·               ╰── assignment not allowed here
//     ╰────
// E0010
//
//   × assignment is not allowed in expression position; assignment is a statement
//     ╭─[parens.baml:49:10]
//  49 │    for ;;i = x) {}
//     ·          ──┬──
//     ·            ╰── assignment not allowed here
//     ╰────
// E0010
//
//   × assignment is not allowed in expression position; assignment is a statement
//     ╭─[parens.baml:51:10]
//  51 │    for ;;i = x {}
//     ·          ──┬──
//     ·            ╰── assignment not allowed here
//     ╰────
