function Factorial(limit: int) -> int {

	let result = 1;


    // used to make the loop break without relying on `break` implementation.
    let should_continue = true;
	while (should_continue) {
		result = result * limit;
		limit = limit - 1;

        if (limit != 0) {
            continue;
        } else {
            should_continue = false;
        }

	}

	result
}

function ContinueNested() -> int {
    while (true) {
        while (false) {
            continue;
        }
        if (false) {
            continue;
        }
    }
    5
}

// Function: Factorial
// 2    0    LOAD_CONST 0        (1)
//
// 6    1    LOAD_CONST 1        (true)
//
// 7    2    LOAD_VAR 3          (should_continue)
//      3    JUMP_IF_FALSE +21   (to 24)
//      4    POP 1               
//
// 8    5    LOAD_VAR 2          (result)
//      6    LOAD_VAR 1          (limit)
//      7    BIN_OP *            
//      8    STORE_VAR 2         (result)
//
// 9    9    LOAD_VAR 1          (limit)
//      10   LOAD_CONST 2        (1)
//      11   BIN_OP -            
//      12   STORE_VAR 1         (limit)
//
// 11   13   LOAD_VAR 1          (limit)
//      14   LOAD_CONST 3        (0)
//      15   CMP_OP !=           
//      16   JUMP_IF_FALSE +4    (to 20)
//      17   POP 1               
//      18   JUMP +5             (to 23)
//      19   JUMP +4             (to 23)
//      20   POP 1               
//
// 14   21   LOAD_CONST 4        (false)
//      22   STORE_VAR 3         (should_continue)
//      23   JUMP -21            (to 2)
//      24   POP 1               
//
// 19   25   LOAD_VAR 2          (result)
//      26   RETURN              
//
// Function: ContinueNested
// 23   0    LOAD_CONST 0        (true)
//      1    JUMP_IF_FALSE +15   (to 16)
//      2    POP 1               
//
// 24   3    LOAD_CONST 1        (false)
//      4    JUMP_IF_FALSE +4    (to 8)
//      5    POP 1               
//      6    JUMP +1             (to 7)
//      7    JUMP -4             (to 3)
//      8    POP 1               
//
// 27   9    LOAD_CONST 2        (false)
//      10   JUMP_IF_FALSE +4    (to 14)
//      11   POP 1               
//      12   JUMP +3             (to 15)
//      13   JUMP +2             (to 15)
//      14   POP 1               
//      15   JUMP -15            (to 0)
//      16   POP 1               
//
// 31   17   LOAD_CONST 3        (5)
//      18   RETURN              
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
