function SumToTen() -> int {
	let s = 0;

	for (let i = 1; i <= 10; i += 1) {
		s += i;
	}

	s
}

function AfterWithBreakContinue() -> int {
	let s = 0;

	for (let i = 0; ; s += i) {
		i += 1;
		if (i > 10) {
			break;
		}
		if (i == 5) {
			continue;
		}
	}

	s
}


function OnlyCond() -> int {
	let s = 0;

	for (; false;) {
	}

	s
}


function Nothing() -> int {
	let s = 0;

	for (;;) {
		break;
	}

	s
}

// Function: SumToTen
// 1   0    LOAD_CONST 0        (0)
//
// 3   1    LOAD_CONST 1        (1)
//     2    LOAD_VAR 2          (i)
//     3    LOAD_CONST 2        (10)
//     4    CMP_OP <=           
//     5    JUMP_IF_FALSE +11   (to 16)
//     6    POP 1               
//     7    LOAD_VAR 1          (s)
//
// 4   8    LOAD_VAR 2          (i)
//     9    BIN_OP +            
//     10   STORE_VAR 1         (s)
//
//      11   LOAD_VAR 2          (i)
//     12   LOAD_CONST 3        (1)
//     13   BIN_OP +            
//     14   STORE_VAR 2         (i)
//     15   JUMP -13            (to 2)
//     16   POP 1               
//     17   POP 1               
//
// 7   18   LOAD_VAR 1          (s)
//     19   RETURN              
//
// Function: AfterWithBreakContinue
// 11   0    LOAD_CONST 0       (0)
//
// 13   1    LOAD_CONST 1       (0)
//      2    LOAD_VAR 2         (i)
//
// 14   3    LOAD_CONST 2       (1)
//      4    BIN_OP +           
//      5    STORE_VAR 2        (i)
//
// 15   6    LOAD_VAR 2         (i)
//      7    LOAD_CONST 3       (10)
//      8    CMP_OP >           
//      9    JUMP_IF_FALSE +4   (to 13)
//      10   POP 1              
//      11   JUMP +16           (to 27)
//      12   JUMP +2            (to 14)
//      13   POP 1              
//
// 18   14   LOAD_VAR 2         (i)
//      15   LOAD_CONST 4       (5)
//      16   CMP_OP ==          
//      17   JUMP_IF_FALSE +4   (to 21)
//      18   POP 1              
//      19   JUMP +3            (to 22)
//      20   JUMP +2            (to 22)
//      21   POP 1              
//
//      22   LOAD_VAR 1         (s)
//      23   LOAD_VAR 2         (i)
//      24   BIN_OP +           
//      25   STORE_VAR 1        (s)
//      26   JUMP -25           (to 1)
//      27   POP 1              
//
// 23   28   LOAD_VAR 1         (s)
//      29   RETURN             
//
// Function: OnlyCond
// 28   0   LOAD_CONST 0       (0)
//
// 30   1   LOAD_CONST 1       (false)
//      2   JUMP_IF_FALSE +3   (to 5)
//      3   POP 1              
//      4   JUMP -3            (to 1)
//      5   POP 1              
//
// 33   6   LOAD_VAR 1         (s)
//      7   RETURN             
//
// Function: Nothing
// 38   0   LOAD_CONST 0   (0)
//      1   JUMP +2        (to 3)
//      2   JUMP -2        (to 0)
//
// 44   3   LOAD_VAR 1     (s)
//      4   RETURN         
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
