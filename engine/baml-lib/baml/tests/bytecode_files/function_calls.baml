// Functions that call other functions
function Add(x: int, y: int) -> int {
    x
}

function Multiply(x: int, y: int) -> int {
    y
}

function Calculate(a: int, b: int, c: int) -> int {
    let sum = Add(a, b);
    let product = Multiply(sum, c);
    product
}

function Nested(x: int) -> int {
    Add(Multiply(x, 2), Add(x, 1))
}

// Function: Add
// 2   0   LOAD_VAR 1   (x)
//     1   RETURN       
//
// Function: Multiply
// 6   0   LOAD_VAR 2   (y)
//     1   RETURN       
//
// Function: Calculate
// 10   0   LOAD_GLOBAL 0   (<fn Add>)
//      1   LOAD_VAR 1      (a)
//      2   LOAD_VAR 2      (b)
//      3   CALL 2          
//
// 11   4   LOAD_GLOBAL 1   (<fn Multiply>)
//      5   LOAD_VAR 4      (sum)
//      6   LOAD_VAR 3      (c)
//      7   CALL 2          
//
// 12   8   LOAD_VAR 5      (product)
//      9   RETURN          
//
// Function: Nested
// 16   0    LOAD_GLOBAL 0   (<fn Add>)
//      1    LOAD_GLOBAL 1   (<fn Multiply>)
//      2    LOAD_VAR 1      (x)
//      3    LOAD_CONST 0    (2)
//      4    CALL 2          
//      5    LOAD_GLOBAL 0   (<fn Add>)
//      6    LOAD_VAR 1      (x)
//      7    LOAD_CONST 1    (1)
//      8    CALL 2          
//      9    CALL 2          
//      10   RETURN          
//
// Class: std::Request with 3 fields
// Function: len