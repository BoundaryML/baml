// Expression functions returning literal values
function ReturnInt() -> int {
    42
}

function ReturnString() -> string {
    "Hello World"
}

function ReturnBool() -> bool {
    true
}

function ReturnFloat() -> float {
    3.14
}

function ReturnArray() -> int[] {
    [1, 2, 3, 4, 5]
}

// Function: ReturnInt
// 2   0   LOAD_CONST 0   (42)
//     1   RETURN         
//
// String: "Hello World"
// Function: ReturnString
// 6   0   LOAD_CONST 0   (Hello World)
//     1   RETURN         
//
// Function: ReturnBool
// 10   0   LOAD_CONST 0   (true)
//      1   RETURN         
//
// Function: ReturnFloat
// 14   0   LOAD_CONST 0   (3.14)
//      1   RETURN         
//
// Function: ReturnArray
// 18   0   LOAD_CONST 0    (1)
//      1   LOAD_CONST 1    (2)
//      2   LOAD_CONST 2    (3)
//      3   LOAD_CONST 3    (4)
//      4   LOAD_CONST 4    (5)
//      5   ALLOC_ARRAY 5   
//      6   RETURN          
//
// Class: std::Request with 3 fields
// Function: std.Array.len
