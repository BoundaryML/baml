fn assertOk() -> int {

	assert 2 + 2 == 4;

	3
}

fn assertNotOk() -> int {
	assert 3 == 1;

	2
}

// Function: assertOk
// 2   0   LOAD_CONST 0   (2)
//     1   LOAD_CONST 1   (2)
//     2   BIN_OP +       
//     3   LOAD_CONST 2   (4)
//     4   CMP_OP ==      
//     5   ASSERT         
//
// 4   6   LOAD_CONST 3   (3)
//     7   RETURN         
//
// Function: assertNotOk
// 8    0   LOAD_CONST 0   (3)
//      1   LOAD_CONST 1   (1)
//      2   CMP_OP ==      
//      3   ASSERT         
//
// 10   4   LOAD_CONST 2   (2)
//      5   RETURN         
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
