function EarlyReturn(x: int) -> int {
   if (x == 42) { return 1; }

   x + 5
}

function WithStack(x: int) -> int {
   let a = 1;

   // NOTE: currently there's no empty returns.

   if (a == 0) { return 0; }

   {
      let b = 1;
      if (a != b) {
         return 0;
      }
   }

   {
      let c = 2;
      let b = 3;
      while (b != c) {
         if (true) {
	          return 0;
	     }
      }
	}

    7
}

function BadValueReturn(x: int) -> string {
   // should error with `return int, expected string`
   return 1;
}

// error: Expected a string value, but received int value `1`.
//   -->  tests/return.baml:36
//    | 
// 35 |    // should error with `return int, expected string`
// 36 |    return 1;
// 37 | }
//    |

//----
//- diagnostics
// Error: Expected `string`, found `int`
//     ╭─[ misc_return.baml:36:11 ]
//     │
//  36 │    return 1;
//     │           ┬  
//     │           ╰── Expected `string`, found `int`
//     │ 
//     │ Note: Error code: E0001
// ────╯
// Error: Statement must end with a semicolon.
//     ╭─[ misc_return.baml:12:4 ]
//     │
//  12 │    if (a == 0) { return 0; }
//     │    ────────────┬────────────  
//     │                ╰────────────── Statement must end with a semicolon.
//     │ 
//     │ Note: Error code: E0028
// ────╯
// Error: Statement must end with a semicolon.
//     ╭─[ misc_return.baml:14:4 ]
//     │
//  14 │ ╭─▶    {
//     ┆ ┆   
//  19 │ ├─▶    }
//     │ │          
//     │ ╰────────── Statement must end with a semicolon.
//     │     
//     │     Note: Error code: E0028
// ────╯
// Error: Statement must end with a semicolon.
//     ╭─[ misc_return.baml:21:4 ]
//     │
//  21 │ ╭─▶    {
//     ┆ ┆   
//  29 │ ├─▶     }
//     │ │           
//     │ ╰─────────── Statement must end with a semicolon.
//     │     
//     │     Note: Error code: E0028
// ────╯
// Error: Statement must end with a semicolon.
//    ╭─[ misc_return.baml:2:4 ]
//    │
//  2 │    if (x == 42) { return 1; }
//    │    ─────────────┬────────────  
//    │                 ╰────────────── Statement must end with a semicolon.
//    │ 
//    │ Note: Error code: E0028
// ───╯
