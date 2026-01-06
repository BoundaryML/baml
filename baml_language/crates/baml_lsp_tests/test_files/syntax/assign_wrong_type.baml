function AssignWrongType(x: int) -> int {

	let y = 3;

	y = "hello";


	42
}

// error: Error validating: Cannot assign string to int
//   -->  assign_wrong_type.baml:5
//    | 
//  4 | 
//  5 | y = "hello";
//  6 | 
//    |

//----
//- diagnostics
// Error: Expected int, found string
//    ╭─[ 0:5:5 ]
//    │
//  5 │     y = "hello";
//    │        ────┬───  
//    │            ╰───── Expected int, found string
//    │ 
//    │ Note: Error code: E0001
// ───╯
