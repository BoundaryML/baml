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
// Class: baml.HttpRequest with 5 fields
// Class: baml.WatchOptions with 2 fields
// Enum baml.HttpMethod
// Function: baml.String.length
//
// Function: baml.Array.length
//
// Function: baml.Array.push
//
// Function: baml.Map.length
//
// Function: baml.Map.has
//
// Function: baml.String.toLowerCase
//
// Function: baml.String.toUpperCase
//
// Function: baml.String.trim
//
// Function: baml.String.includes
//
// Function: baml.String.startsWith
//
// Function: baml.String.endsWith
//
// Function: baml.String.split
//
// Function: baml.String.substring
//
// Function: baml.String.replace
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
// Function: baml.deep_equals
//
// Function: baml.unstable.string
//
// Function: baml.fetch_as
