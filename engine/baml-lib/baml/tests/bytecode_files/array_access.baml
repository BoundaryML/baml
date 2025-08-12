// Array access tests
function GetFirstElement(arr: int[]) -> int {
    arr[0]
}

function GetElementAtIndex(arr: string[], index: int) -> string {
    arr[index]
}

function GetNestedElement(matrix: int[][]) -> int {
    matrix[1][2]
}

function ArrayAccessWithVariable(arr: float[], idx: int) -> float {
    let result = arr[idx];
    result
}

// Function: GetFirstElement
// 2   0   LOAD_VAR 1           (arr)
//     1   LOAD_CONST 0         (0)
//     2   LOAD_ARRAY_ELEMENT   
//     3   RETURN               
//
// Function: GetElementAtIndex
// 6   0   LOAD_VAR 1           (arr)
//     1   LOAD_VAR 2           (index)
//     2   LOAD_ARRAY_ELEMENT   
//     3   RETURN               
//
// Function: GetNestedElement
// 10   0   LOAD_VAR 1           (matrix)
//      1   LOAD_CONST 0         (1)
//      2   LOAD_ARRAY_ELEMENT   
//      3   LOAD_CONST 1         (2)
//      4   LOAD_ARRAY_ELEMENT   
//      5   RETURN               
//
// Function: ArrayAccessWithVariable
// 14   0   LOAD_VAR 1           (arr)
//      1   LOAD_VAR 2           (idx)
//      2   LOAD_ARRAY_ELEMENT   
//
// 15   3   LOAD_VAR 3           (result)
//      4   RETURN               
//
// Class: std::Request with 3 fields