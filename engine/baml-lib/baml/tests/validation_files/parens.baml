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
