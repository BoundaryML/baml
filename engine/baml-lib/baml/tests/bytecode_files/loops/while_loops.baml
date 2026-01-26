function GCD(a: int, b: int) -> int {

	while (a != b) {
	   if (a > b) {
	   	a = a - b;
	   } else {
	   	b = b - a;
	   }
	}

	a
}

function WhileWithScopes() -> int {

	while (true) {
		let a = 0;

		// NOTE: seems like nested blocks are not properly parsed?
		// {
		// 	let x = 0;
		// 	if a != x {
		// 		continue;
		// 	}
		// }

		// {
		// 	let y = 0;
		// 	let x = 1;
		// 	if x != y {
		// 		break;
		// 	}
		// }



		if (a > 0) {
			let x = 0; // bogus variable to have different stack lengths @ continue/break
			continue;
		}
		if (a < 0) {
			let y = 0;
			let x = 0;
			break;
		}

		// NOTE: there's a compiler bug wrt PopReplace where it will emit PopReplace when the last block's expression.
		// Avoid that by using a statement. Fixed by #2323.
		a = 1;
	}

	3
}

// Function: GCD
// 2    0    LOAD_VAR 1          (a)
//      1    LOAD_VAR 2          (b)
//      2    CMP_OP !=           
//      3    JUMP_IF_FALSE +18   (to 21)
//      4    POP 1               
//
// 3    5    LOAD_VAR 1          (a)
//      6    LOAD_VAR 2          (b)
//      7    CMP_OP >            
//      8    JUMP_IF_FALSE +7    (to 15)
//      9    POP 1               
//
// 4    10   LOAD_VAR 1          (a)
//      11   LOAD_VAR 2          (b)
//      12   BIN_OP -            
//      13   STORE_VAR 1         (a)
//      14   JUMP +6             (to 20)
//      15   POP 1               
//
// 6    16   LOAD_VAR 2          (b)
//      17   LOAD_VAR 1          (a)
//      18   BIN_OP -            
//      19   STORE_VAR 2         (b)
//      20   JUMP -20            (to 0)
//      21   POP 1               
//
// 10   22   LOAD_VAR 1          (a)
//      23   RETURN              
//
// Function: WhileWithScopes
// 15   0    LOAD_CONST 0        (true)
//      1    JUMP_IF_FALSE +30   (to 31)
//      2    POP 1               
//
// 16   3    LOAD_CONST 1        (0)
//
// 36   4    LOAD_VAR 1          (a)
//      5    LOAD_CONST 2        (0)
//      6    CMP_OP >            
//      7    JUMP_IF_FALSE +7    (to 14)
//      8    POP 1               
//
// 37   9    LOAD_CONST 3        (0)
//      10   POP 2               
//      11   JUMP +19            (to 30)
//      12   POP 1               
//      13   JUMP +2             (to 15)
//      14   POP 1               
//
// 40   15   LOAD_VAR 1          (a)
//      16   LOAD_CONST 4        (0)
//      17   CMP_OP <            
//      18   JUMP_IF_FALSE +8    (to 26)
//      19   POP 1               
//
// 41   20   LOAD_CONST 5        (0)
//
// 42   21   LOAD_CONST 6        (0)
//      22   POP 3               
//      23   JUMP +9             (to 32)
//      24   POP 2               
//      25   JUMP +2             (to 27)
//      26   POP 1               
//
// 48   27   LOAD_CONST 7        (1)
//      28   STORE_VAR 1         (a)
//      29   POP 1               
//      30   JUMP -30            (to 0)
//      31   POP 1               
//
// 51   32   LOAD_CONST 8        (3)
//      33   RETURN              
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
