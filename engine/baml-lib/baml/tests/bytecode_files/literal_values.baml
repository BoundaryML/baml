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

// error: Expected a string value, but received float value `3.14`.
//   -->  test.baml:15
//    | 
// 14 | function ReturnFloat() -> float {
// 15 |     3.14
//    | 
