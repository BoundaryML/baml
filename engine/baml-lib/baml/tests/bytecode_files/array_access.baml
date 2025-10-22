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
// Class: baml.Request with 3 fields
// Class: baml.WatchOptions with 2 fields
// Enum baml.HttpMethod
// Function: baml.Array.length
//
// Function: baml.Map.length
//
// Function: baml.Map.has
//
// Function: baml.media.image.from_url
//
// Function: baml.media.audio.from_url
//
// Function: baml.media.video.from_url
//
// Function: baml.media.pdf.from_url
//
// Function: baml.media.image.from_base64
//
// Function: baml.media.audio.from_base64
//
// Function: baml.media.video.from_base64
//
// Function: baml.media.pdf.from_base64
//
// Function: baml.media.image.is_url
//
// Function: baml.media.video.is_url
//
// Function: baml.media.audio.is_url
//
// Function: baml.media.pdf.is_url
//
// Function: baml.media.image.is_base64
//
// Function: baml.media.video.is_base64
//
// Function: baml.media.audio.is_base64
//
// Function: baml.media.pdf.is_base64
//
// Function: baml.media.image.as_url
//
// Function: baml.media.video.as_url
//
// Function: baml.media.audio.as_url
//
// Function: baml.media.pdf.as_url
//
// Function: baml.media.image.as_base64
//
// Function: baml.media.video.as_base64
//
// Function: baml.media.audio.as_base64
//
// Function: baml.media.pdf.as_base64
//
// Function: baml.media.image.mime
//
// Function: baml.media.video.mime
//
// Function: baml.media.audio.mime
//
// Function: baml.media.pdf.mime
//
// Function: env.get
//
// Function: baml.deep_copy
//
// Function: baml.unstable.string
//
// Function: baml.fetch_as
