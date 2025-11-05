function EarlyReturn(x: int) -> int {
   if (x == 42) { return 1; }

   x + 5
}

function WithStack(x: int) -> int {
   let a = 1;

   // NOTE: currently there's no empty returns.

   if (a == 0) { return 0; }

   {
      let b = 1;
      if (a != b) {
         return 0;
      }
   }

   {
      let c = 2;
      let b = 3;
      while (b != c) {
         if (true) {
	          return 0;
	     }
      }
	}

    7
}

// Function: EarlyReturn
// 1   0    LOAD_VAR 1         (x)
//     1    LOAD_CONST 0       (42)
//     2    CMP_OP ==          
//     3    JUMP_IF_FALSE +5   (to 8)
//     4    POP 1              
//     5    LOAD_CONST 1       (1)
//     6    RETURN             
//     7    JUMP +2            (to 9)
//     8    POP 1              
//
// 3   9    LOAD_VAR 1         (x)
//     10   LOAD_CONST 2       (5)
//     11   BIN_OP +           
//     12   RETURN             
//
// Function: WithStack
// 7    0    LOAD_CONST 0        (1)
//
// 11   1    LOAD_VAR 2          (a)
//      2    LOAD_CONST 1        (0)
//      3    CMP_OP ==           
//      4    JUMP_IF_FALSE +5    (to 9)
//      5    POP 1               
//      6    LOAD_CONST 2        (0)
//      7    RETURN              
//      8    JUMP +2             (to 10)
//      9    POP 1               
//
// 14   10   LOAD_CONST 3        (1)
//
// 15   11   LOAD_VAR 2          (a)
//      12   LOAD_VAR 3          (b)
//      13   CMP_OP !=           
//      14   JUMP_IF_FALSE +5    (to 19)
//      15   POP 1               
//
// 16   16   LOAD_CONST 4        (0)
//      17   RETURN              
//      18   JUMP +2             (to 20)
//      19   POP 1               
//      20   POP 1               
//
// 21   21   LOAD_CONST 5        (2)
//
// 22   22   LOAD_CONST 6        (3)
//
// 23   23   LOAD_VAR 4          (b)
//      24   LOAD_VAR 3          (c)
//      25   CMP_OP !=           
//      26   JUMP_IF_FALSE +10   (to 36)
//      27   POP 1               
//
// 24   28   LOAD_CONST 7        (true)
//      29   JUMP_IF_FALSE +5    (to 34)
//      30   POP 1               
//
// 25   31   LOAD_CONST 8        (0)
//      32   RETURN              
//      33   JUMP +2             (to 35)
//      34   POP 1               
//      35   JUMP -12            (to 23)
//      36   POP 1               
//      37   POP 2               
//
// 30   38   LOAD_CONST 9        (7)
//      39   RETURN              
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
