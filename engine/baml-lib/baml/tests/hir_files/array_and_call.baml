function Helper(x: int) -> int {
    x
}

function TestArray() -> int[] {
    let arr = [1, 2, 3];
    let result = Helper(42);
    [arr, result]
}

function ArrayAccess() -> int {
    let arr = [1, 2, 3];
    arr[1]
}

// error: Expected a array value, but received array value `[arr, result]`.
//   -->  test.baml:8
//    | 
//  7 |     let result = Helper(42);
//  8 |     [arr, result]
//    | 
