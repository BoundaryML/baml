fn Sum(xs: int[]) -> int {
	let mut result = 0;

	for x in xs {
		result += x;
	}

	result
}

fn ForWithBreak(xs: int[]) -> int {
	let mut result = 0;

	for x in xs {
		if x > 10 {
			break;
		}
		result += x;
	}

	result
}

fn ForWithContinue(xs: int[]) -> int {
	let mut result = 0;

	for x in xs {
		if x > 10 {
			continue;
		}
		result += x;
	}

	result
}

fn NestedFor(as: int[], bs: int[]) -> int {

	let mut result = 0;

	for a in as {
		for b in bs {
			result += a * b;
		}
	}

	result
}

// Function: Sum
// 1   0    LOAD_CONST 0         (0)
//
// 3   1    LOAD_VAR 1           (xs)
//     2    ARRAY_LENGTH         
//     3    LOAD_CONST 0         (0)
//     4    LOAD_VAR 5           (__baml for loop index0)
//     5    LOAD_VAR 4           (__baml for loop array length0)
//     6    CMP_OP <             
//     7    JUMP_IF_FALSE +17    (to 24)
//     8    POP 1                
//     9    LOAD_VAR 3           (__baml for loop iterated array0)
//     10   LOAD_VAR 5           (__baml for loop index0)
//     11   LOAD_ARRAY_ELEMENT   
//     12   LOAD_VAR 5           (__baml for loop index0)
//     13   LOAD_CONST 1         (1)
//     14   BIN_OP +             
//     15   STORE_VAR 5          (__baml for loop index0)
//     16   LOAD_VAR 2           (result)
//
// 4   17   LOAD_VAR 6           (x)
//     18   BIN_OP +             
//     19   STORE_VAR 2          (result)
//     20   POP 1                
//     21   JUMP -17             (to 4)
//     22   POP 1                
//     23   JUMP +2              (to 25)
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
//      2    ARRAY_LENGTH         
//      3    LOAD_CONST 0         (0)
//      4    LOAD_VAR 5           (__baml for loop index1)
//      5    LOAD_VAR 4           (__baml for loop array length1)
//      6    CMP_OP <             
//      7    JUMP_IF_FALSE +25    (to 32)
//      8    POP 1                
//      9    LOAD_VAR 3           (__baml for loop iterated array1)
//      10   LOAD_VAR 5           (__baml for loop index1)
//      11   LOAD_ARRAY_ELEMENT   
//      12   LOAD_VAR 5           (__baml for loop index1)
//      13   LOAD_CONST 1         (1)
//      14   BIN_OP +             
//      15   STORE_VAR 5          (__baml for loop index1)
//
// 14   16   LOAD_VAR 6           (x)
//      17   LOAD_CONST 2         (10)
//      18   CMP_OP >             
//      19   JUMP_IF_FALSE +4     (to 23)
//      20   POP 1                
//      21   JUMP +9              (to 30)
//      22   JUMP +2              (to 24)
//      23   POP 1                
//      24   LOAD_VAR 2           (result)
//
// 17   25   LOAD_VAR 6           (x)
//      26   BIN_OP +             
//      27   STORE_VAR 2          (result)
//      28   POP 1                
//      29   JUMP -25             (to 4)
//      30   POP 1                
//      31   JUMP +2              (to 33)
//      32   POP 1                
//      33   POP 3                
//
// 20   34   LOAD_VAR 2           (result)
//      35   RETURN               
//
// Function: ForWithContinue
// 24   0    LOAD_CONST 0         (0)
//
// 26   1    LOAD_VAR 1           (xs)
//      2    ARRAY_LENGTH         
//      3    LOAD_CONST 0         (0)
//      4    LOAD_VAR 5           (__baml for loop index2)
//      5    LOAD_VAR 4           (__baml for loop array length2)
//      6    CMP_OP <             
//      7    JUMP_IF_FALSE +25    (to 32)
//      8    POP 1                
//      9    LOAD_VAR 3           (__baml for loop iterated array2)
//      10   LOAD_VAR 5           (__baml for loop index2)
//      11   LOAD_ARRAY_ELEMENT   
//      12   LOAD_VAR 5           (__baml for loop index2)
//      13   LOAD_CONST 1         (1)
//      14   BIN_OP +             
//      15   STORE_VAR 5          (__baml for loop index2)
//
// 27   16   LOAD_VAR 6           (x)
//      17   LOAD_CONST 2         (10)
//      18   CMP_OP >             
//      19   JUMP_IF_FALSE +4     (to 23)
//      20   POP 1                
//      21   JUMP +7              (to 28)
//      22   JUMP +2              (to 24)
//      23   POP 1                
//      24   LOAD_VAR 2           (result)
//
// 30   25   LOAD_VAR 6           (x)
//      26   BIN_OP +             
//      27   STORE_VAR 2          (result)
//      28   POP 1                
//      29   JUMP -25             (to 4)
//      30   POP 1                
//      31   JUMP +2              (to 33)
//      32   POP 1                
//      33   POP 3                
//
// 33   34   LOAD_VAR 2           (result)
//      35   RETURN               
//
// Function: NestedFor
// 38   0    LOAD_CONST 0         (0)
//
// 40   1    LOAD_VAR 1           (as)
//      2    ARRAY_LENGTH         
//      3    LOAD_CONST 0         (0)
//      4    LOAD_VAR 6           (__baml for loop index3)
//      5    LOAD_VAR 5           (__baml for loop array length3)
//      6    CMP_OP <             
//      7    JUMP_IF_FALSE +40    (to 47)
//      8    POP 1                
//      9    LOAD_VAR 4           (__baml for loop iterated array3)
//      10   LOAD_VAR 6           (__baml for loop index3)
//      11   LOAD_ARRAY_ELEMENT   
//      12   LOAD_VAR 6           (__baml for loop index3)
//      13   LOAD_CONST 1         (1)
//      14   BIN_OP +             
//      15   STORE_VAR 6          (__baml for loop index3)
//
// 41   16   LOAD_VAR 2           (bs)
//      17   ARRAY_LENGTH         
//      18   LOAD_CONST 0         (0)
//      19   LOAD_VAR 10          (__baml for loop index4)
//      20   LOAD_VAR 9           (__baml for loop array length4)
//      21   CMP_OP <             
//      22   JUMP_IF_FALSE +19    (to 41)
//      23   POP 1                
//      24   LOAD_VAR 8           (__baml for loop iterated array4)
//      25   LOAD_VAR 10          (__baml for loop index4)
//      26   LOAD_ARRAY_ELEMENT   
//      27   LOAD_VAR 10          (__baml for loop index4)
//      28   LOAD_CONST 1         (1)
//      29   BIN_OP +             
//      30   STORE_VAR 10         (__baml for loop index4)
//      31   LOAD_VAR 3           (result)
//
// 42   32   LOAD_VAR 7           (a)
//      33   LOAD_VAR 11          (b)
//      34   BIN_OP *             
//      35   BIN_OP +             
//      36   STORE_VAR 3          (result)
//      37   POP 1                
//      38   JUMP -19             (to 19)
//      39   POP 1                
//      40   JUMP +2              (to 42)
//      41   POP 1                
//      42   POP 3                
//      43   POP 1                
//      44   JUMP -40             (to 4)
//      45   POP 5                
//      46   JUMP +2              (to 48)
//      47   POP 1                
//      48   POP 3                
//
// 46   49   LOAD_VAR 3           (result)
//      50   RETURN               
//
// Class: std::Request with 3 fields