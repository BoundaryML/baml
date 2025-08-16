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
//     2    LOAD_GLOBAL 5        (<fn len>)
//     3    LOAD_VAR 3           (__baml for loop iterated array 0)
//     4    CALL 1               
//     5    LOAD_CONST 0         (0)
//     6    LOAD_VAR 5           (__baml for loop index 0)
//     7    LOAD_VAR 4           (__baml for loop array length 0)
//     8    CMP_OP <             
//     9    JUMP_IF_FALSE +17    (to 26)
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
//     25   JUMP +2              (to 27)
//     26   POP 1                
//     27   POP 3                
//
// 7   28   LOAD_VAR 2           (result)
//     29   RETURN               
//
// Function: ForWithBreak
// 11   0    LOAD_CONST 0         (0)
//
// 13   1    LOAD_VAR 1           (xs)
//      2    LOAD_GLOBAL 5        (<fn len>)
//      3    LOAD_VAR 3           (__baml for loop iterated array 1)
//      4    CALL 1               
//      5    LOAD_CONST 0         (0)
//      6    LOAD_VAR 5           (__baml for loop index 1)
//      7    LOAD_VAR 4           (__baml for loop array length 1)
//      8    CMP_OP <             
//      9    JUMP_IF_FALSE +25    (to 34)
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
//      21   JUMP_IF_FALSE +4     (to 25)
//      22   POP 1                
//      23   JUMP +9              (to 32)
//      24   JUMP +2              (to 26)
//      25   POP 1                
//      26   LOAD_VAR 2           (result)
//
// 17   27   LOAD_VAR 6           (x)
//      28   BIN_OP +             
//      29   STORE_VAR 2          (result)
//      30   POP 1                
//      31   JUMP -25             (to 6)
//      32   POP 1                
//      33   JUMP +2              (to 35)
//      34   POP 1                
//      35   POP 3                
//
// 20   36   LOAD_VAR 2           (result)
//      37   RETURN               
//
// Function: ForWithContinue
// 24   0    LOAD_CONST 0         (0)
//
// 26   1    LOAD_VAR 1           (xs)
//      2    LOAD_GLOBAL 5        (<fn len>)
//      3    LOAD_VAR 3           (__baml for loop iterated array 2)
//      4    CALL 1               
//      5    LOAD_CONST 0         (0)
//      6    LOAD_VAR 5           (__baml for loop index 2)
//      7    LOAD_VAR 4           (__baml for loop array length 2)
//      8    CMP_OP <             
//      9    JUMP_IF_FALSE +25    (to 34)
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
//      21   JUMP_IF_FALSE +4     (to 25)
//      22   POP 1                
//      23   JUMP +7              (to 30)
//      24   JUMP +2              (to 26)
//      25   POP 1                
//      26   LOAD_VAR 2           (result)
//
// 30   27   LOAD_VAR 6           (x)
//      28   BIN_OP +             
//      29   STORE_VAR 2          (result)
//      30   POP 1                
//      31   JUMP -25             (to 6)
//      32   POP 1                
//      33   JUMP +2              (to 35)
//      34   POP 1                
//      35   POP 3                
//
// 33   36   LOAD_VAR 2           (result)
//      37   RETURN               
//
// Function: NestedFor
// 38   0    LOAD_CONST 0         (0)
//
// 40   1    LOAD_VAR 1           (as)
//      2    LOAD_GLOBAL 5        (<fn len>)
//      3    LOAD_VAR 4           (__baml for loop iterated array 3)
//      4    CALL 1               
//      5    LOAD_CONST 0         (0)
//      6    LOAD_VAR 6           (__baml for loop index 3)
//      7    LOAD_VAR 5           (__baml for loop array length 3)
//      8    CMP_OP <             
//      9    JUMP_IF_FALSE +42    (to 51)
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
//      19   LOAD_GLOBAL 5        (<fn len>)
//      20   LOAD_VAR 8           (__baml for loop iterated array 4)
//      21   CALL 1               
//      22   LOAD_CONST 0         (0)
//      23   LOAD_VAR 10          (__baml for loop index 4)
//      24   LOAD_VAR 9           (__baml for loop array length 4)
//      25   CMP_OP <             
//      26   JUMP_IF_FALSE +19    (to 45)
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
//      44   JUMP +2              (to 46)
//      45   POP 1                
//      46   POP 3                
//      47   POP 1                
//      48   JUMP -42             (to 6)
//      49   POP 5                
//      50   JUMP +2              (to 52)
//      51   POP 1                
//      52   POP 3                
//
// 46   53   LOAD_VAR 3           (result)
//      54   RETURN               
//
// Class: std::Request with 3 fields
// Function: len