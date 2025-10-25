function Sum(xs: int[]) -> int {
	let result = 0;

	for (let x in xs) {
		result += x;
	}

	result
}

function ForWithBreak(xs: int[]) -> int {
	let result = 0;

	for (let x in xs) {
		if (x > 10) {
			break;
		}
		result += x;
	}

	result
}

function ForWithContinue(xs: int[]) -> int {
	let result = 0;

	for (let x in xs) {
		if (x > 10) {
			continue;
		}
		result += x;
	}

	result
}

function NestedFor(as: int[], bs: int[]) -> int {

	let result = 0;

	for (let a in as) {
		for (let b in bs) {
			result += a * b;
		}
	}

	result
}

// Function: Sum
// 1   0    LOAD_CONST 0         (0)
//
// 3   1    LOAD_VAR 1           (xs)
//     2    LOAD_GLOBAL 8        (<fn baml.Array.length>)
//     3    LOAD_VAR 3           (__baml for loop iterated array 0)
//     4    CALL 1               
//     5    LOAD_CONST 0         (0)
//     6    LOAD_VAR 5           (__baml for loop index 0)
//     7    LOAD_VAR 4           (__baml for loop array length 0)
//     8    CMP_OP <             
//     9    JUMP_IF_FALSE +15    (to 24)
//     10   POP 1                
//     11   LOAD_VAR 3           (__baml for loop iterated array 0)
//     12   LOAD_VAR 5           (__baml for loop index 0)
//     13   LOAD_ARRAY_ELEMENT   
//     14   LOAD_VAR 5           (__baml for loop index 0)
//     15   LOAD_CONST 1         (1)
//     16   BIN_OP +             
//     17   STORE_VAR 5          (__baml for loop index 0)
//     18   LOAD_VAR 2           (result)
//
// 4   19   LOAD_VAR 6           (x)
//     20   BIN_OP +             
//     21   STORE_VAR 2          (result)
//     22   POP 1                
//     23   JUMP -17             (to 6)
//     24   POP 1                
//     25   POP 3                
//
// 7   26   LOAD_VAR 2           (result)
//     27   RETURN               
//
// Function: ForWithBreak
// 11   0    LOAD_CONST 0         (0)
//
// 13   1    LOAD_VAR 1           (xs)
//      2    LOAD_GLOBAL 8        (<fn baml.Array.length>)
//      3    LOAD_VAR 3           (__baml for loop iterated array 1)
//      4    CALL 1               
//      5    LOAD_CONST 0         (0)
//      6    LOAD_VAR 5           (__baml for loop index 1)
//      7    LOAD_VAR 4           (__baml for loop array length 1)
//      8    CMP_OP <             
//      9    JUMP_IF_FALSE +24    (to 33)
//      10   POP 1                
//      11   LOAD_VAR 3           (__baml for loop iterated array 1)
//      12   LOAD_VAR 5           (__baml for loop index 1)
//      13   LOAD_ARRAY_ELEMENT   
//      14   LOAD_VAR 5           (__baml for loop index 1)
//      15   LOAD_CONST 1         (1)
//      16   BIN_OP +             
//      17   STORE_VAR 5          (__baml for loop index 1)
//
// 14   18   LOAD_VAR 6           (x)
//      19   LOAD_CONST 2         (10)
//      20   CMP_OP >             
//      21   JUMP_IF_FALSE +5     (to 26)
//      22   POP 1                
//      23   POP 1                
//      24   JUMP +10             (to 34)
//      25   JUMP +2              (to 27)
//      26   POP 1                
//      27   LOAD_VAR 2           (result)
//
// 17   28   LOAD_VAR 6           (x)
//      29   BIN_OP +             
//      30   STORE_VAR 2          (result)
//      31   POP 1                
//      32   JUMP -26             (to 6)
//      33   POP 1                
//      34   POP 3                
//
// 20   35   LOAD_VAR 2           (result)
//      36   RETURN               
//
// Function: ForWithContinue
// 24   0    LOAD_CONST 0         (0)
//
// 26   1    LOAD_VAR 1           (xs)
//      2    LOAD_GLOBAL 8        (<fn baml.Array.length>)
//      3    LOAD_VAR 3           (__baml for loop iterated array 2)
//      4    CALL 1               
//      5    LOAD_CONST 0         (0)
//      6    LOAD_VAR 5           (__baml for loop index 2)
//      7    LOAD_VAR 4           (__baml for loop array length 2)
//      8    CMP_OP <             
//      9    JUMP_IF_FALSE +24    (to 33)
//      10   POP 1                
//      11   LOAD_VAR 3           (__baml for loop iterated array 2)
//      12   LOAD_VAR 5           (__baml for loop index 2)
//      13   LOAD_ARRAY_ELEMENT   
//      14   LOAD_VAR 5           (__baml for loop index 2)
//      15   LOAD_CONST 1         (1)
//      16   BIN_OP +             
//      17   STORE_VAR 5          (__baml for loop index 2)
//
// 27   18   LOAD_VAR 6           (x)
//      19   LOAD_CONST 2         (10)
//      20   CMP_OP >             
//      21   JUMP_IF_FALSE +5     (to 26)
//      22   POP 1                
//      23   POP 1                
//      24   JUMP +8              (to 32)
//      25   JUMP +2              (to 27)
//      26   POP 1                
//      27   LOAD_VAR 2           (result)
//
// 30   28   LOAD_VAR 6           (x)
//      29   BIN_OP +             
//      30   STORE_VAR 2          (result)
//      31   POP 1                
//      32   JUMP -26             (to 6)
//      33   POP 1                
//      34   POP 3                
//
// 33   35   LOAD_VAR 2           (result)
//      36   RETURN               
//
// Function: NestedFor
// 38   0    LOAD_CONST 0         (0)
//
// 40   1    LOAD_VAR 1           (as)
//      2    LOAD_GLOBAL 8        (<fn baml.Array.length>)
//      3    LOAD_VAR 4           (__baml for loop iterated array 3)
//      4    CALL 1               
//      5    LOAD_CONST 0         (0)
//      6    LOAD_VAR 6           (__baml for loop index 3)
//      7    LOAD_VAR 5           (__baml for loop array length 3)
//      8    CMP_OP <             
//      9    JUMP_IF_FALSE +38    (to 47)
//      10   POP 1                
//      11   LOAD_VAR 4           (__baml for loop iterated array 3)
//      12   LOAD_VAR 6           (__baml for loop index 3)
//      13   LOAD_ARRAY_ELEMENT   
//      14   LOAD_VAR 6           (__baml for loop index 3)
//      15   LOAD_CONST 1         (1)
//      16   BIN_OP +             
//      17   STORE_VAR 6          (__baml for loop index 3)
//
// 41   18   LOAD_VAR 2           (bs)
//      19   LOAD_GLOBAL 8        (<fn baml.Array.length>)
//      20   LOAD_VAR 8           (__baml for loop iterated array 4)
//      21   CALL 1               
//      22   LOAD_CONST 0         (0)
//      23   LOAD_VAR 10          (__baml for loop index 4)
//      24   LOAD_VAR 9           (__baml for loop array length 4)
//      25   CMP_OP <             
//      26   JUMP_IF_FALSE +17    (to 43)
//      27   POP 1                
//      28   LOAD_VAR 8           (__baml for loop iterated array 4)
//      29   LOAD_VAR 10          (__baml for loop index 4)
//      30   LOAD_ARRAY_ELEMENT   
//      31   LOAD_VAR 10          (__baml for loop index 4)
//      32   LOAD_CONST 1         (1)
//      33   BIN_OP +             
//      34   STORE_VAR 10         (__baml for loop index 4)
//      35   LOAD_VAR 3           (result)
//
// 42   36   LOAD_VAR 7           (a)
//      37   LOAD_VAR 11          (b)
//      38   BIN_OP *             
//      39   BIN_OP +             
//      40   STORE_VAR 3          (result)
//      41   POP 1                
//      42   JUMP -19             (to 23)
//      43   POP 1                
//      44   POP 3                
//      45   POP 1                
//      46   JUMP -40             (to 6)
//      47   POP 1                
//      48   POP 3                
//
// 46   49   LOAD_VAR 3           (result)
//      50   RETURN               
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
