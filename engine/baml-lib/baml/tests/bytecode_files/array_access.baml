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
// Enum std::HttpMethod
// Function: std.Array.len
//
// Function: std.Map.len
//
// Function: std.Map.contains
//
// Function: std.media.image.from_url
//
// Function: std.media.audio.from_url
//
// Function: std.media.video.from_url
//
// Function: std.media.pdf.from_url
//
// Function: std.media.image.from_base64
//
// Function: std.media.audio.from_base64
//
// Function: std.media.video.from_base64
//
// Function: std.media.pdf.from_base64
//
// Function: std.media.image.is_url
//
// Function: std.media.video.is_url
//
// Function: std.media.audio.is_url
//
// Function: std.media.pdf.is_url
//
// Function: std.media.image.is_base64
//
// Function: std.media.video.is_base64
//
// Function: std.media.audio.is_base64
//
// Function: std.media.pdf.is_base64
//
// Function: std.media.image.as_url
//
// Function: std.media.video.as_url
//
// Function: std.media.audio.as_url
//
// Function: std.media.pdf.as_url
//
// Function: std.media.image.as_base64
//
// Function: std.media.video.as_base64
//
// Function: std.media.audio.as_base64
//
// Function: std.media.pdf.as_base64
//
// Function: std.media.image.mime
//
// Function: std.media.video.mime
//
// Function: std.media.audio.mime
//
// Function: std.media.pdf.mime
