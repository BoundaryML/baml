function GCD(a: int, b: int) -> int {

	while (a != b) {
	   if (a > b) {
	   	a = a - b;
	   } else {
	   	b = b - a;
	   }
	}

	a
}

function Foo() -> int {

    let x = true;
    let y = false;

    let a = 0;

    while (a < 10) {
        let b = if (x) {
            let c = 5;
            5
         } else if (y) {
            6
         } else {
            7
         };

         a = a + 1;
    }

    3
}

//----
//- diagnostics
// Error: Statement must end with a semicolon.
//     ╭─[ loops_while_loops.baml:21:5 ]
//     │
//  21 │ ╭─▶     while (a < 10) {
//     ┆ ┆   
//  32 │ ├─▶     }
//     │ │           
//     │ ╰─────────── Statement must end with a semicolon.
//     │     
//     │     Note: Error code: E0028
// ────╯
// Error: Statement must end with a semicolon.
//    ╭─[ loops_while_loops.baml:3:2 ]
//    │
//  3 │ ╭─▶     while (a != b) {
//    ┆ ┆   
//  9 │ ├─▶     }
//    │ │           
//    │ ╰─────────── Statement must end with a semicolon.
//    │     
//    │     Note: Error code: E0028
// ───╯
