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
