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
