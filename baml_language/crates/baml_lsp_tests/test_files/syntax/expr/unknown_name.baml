function Go(x: int) -> int {
    let y = a;
    y
}

function Go2(x: int) -> int {
    let z = x;
    let a = z;
    Go(z,a,b)
}

function Go3(x:int) -> int {
    Unknown(x)
}

// error: Unknown variable a
//   -->  expr/unknown_name.baml:2
//    | 
//  1 | function Go(x: int) -> int {
//  2 |     let y = a;
//    | 
// error: Unknown variable b
//   -->  expr/unknown_name.baml:9
//    | 
//  8 |     let a = z;
//  9 |     Go(z,a,b)
//    | 
// error: Unknown function Unknown
//   -->  expr/unknown_name.baml:13
//    | 
// 12 | function Go3(x:int) -> int {
// 13 |     Unknown(x)
//    | 
// error: Error validating: Unknown variable a
//   -->  expr/unknown_name.baml:2
//    | 
//  1 | function Go(x: int) -> int {
//  2 |     let y = a;
//    | 
// error: Error validating: Type mismatch in argument
//   -->  expr/unknown_name.baml:9
//    | 
//  8 |     let a = z;
//  9 |     Go(z,a,b)
//    | 
// error: Error validating: Unknown variable b
//   -->  expr/unknown_name.baml:9
//    | 
//  8 |     let a = z;
//  9 |     Go(z,a,b)
//    | 
// error: Error validating: Function Go expects 1 arguments, got 3
//   -->  expr/unknown_name.baml:9
//    | 
//  8 |     let a = z;
//  9 |     Go(z,a,b)
//    | 
// error: Error validating: Unknown function Unknown
//   -->  expr/unknown_name.baml:13
//    | 
// 12 | function Go3(x:int) -> int {
// 13 |     Unknown(x)
//    |

//----
//- diagnostics
// Error: Unknown variable Unknown
//     ╭─[ 0:13:5 ]
//     │
//  13 │     Unknown(x)
//     │     ───┬───  
//     │        ╰───── Unknown variable Unknown
//     │ 
//     │ Note: Error code: E0003
// ────╯
// Error: Unknown variable b
//    ╭─[ 0:9:12 ]
//    │
//  9 │     Go(z,a,b)
//    │            ┬  
//    │            ╰── Unknown variable b
//    │ 
//    │ Note: Error code: E0003
// ───╯
// Error: Expected 1 arguments, found 3
//    ╭─[ 0:9:5 ]
//    │
//  9 │     Go(z,a,b)
//    │     ────┬────  
//    │         ╰────── Expected 1 arguments, found 3
//    │ 
//    │ Note: Error code: E0005
// ───╯
