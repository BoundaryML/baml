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