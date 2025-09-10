fn DeclareMutableInFunction(x: int) -> int {

	let y = 3;

	y = 5;

	y
}

fn MutableInArg(x: int) -> int {
	x = 3;
	x
}

// Function: DeclareMutableInFunction
// 2   0   LOAD_CONST 0   (3)
//
// 4   1   LOAD_CONST 1   (5)
//     2   STORE_VAR 2    (y)
//
// 6   3   LOAD_VAR 2     (y)
//     4   RETURN         
//
// Function: MutableInArg
// 10   0   LOAD_CONST 0   (3)
//      1   STORE_VAR 1    (x)
//
// 11   2   LOAD_VAR 1     (x)
//      3   RETURN         
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
