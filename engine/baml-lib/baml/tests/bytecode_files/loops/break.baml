fn Factorial(mut limit: int) -> int {

	let mut result = 1;


	while true {
		if limit == 0 {
			break;
		}
		result = result * limit;
		limit = limit - 1;
	}

	result
}

fn Nested() -> int {
    let mut a = 5;
    while true {
        while true {
            a = a + 1;
            break;
        }
        a = a + 1;
        break;
    }
     // should be 2
    a
}

// Function: Factorial
// 2    0    LOAD_CONST 0        (1)
//
// 5    1    LOAD_CONST 1        (true)
//      2    JUMP_IF_FALSE +19   (to 21)
//      3    POP 1               
//
// 6    4    LOAD_VAR 1          (limit)
//      5    LOAD_CONST 2        (0)
//      6    CMP_OP ==           
//      7    JUMP_IF_FALSE +4    (to 11)
//      8    POP 1               
//      9    JUMP +13            (to 22)
//      10   JUMP +2             (to 12)
//      11   POP 1               
//
// 9    12   LOAD_VAR 2          (result)
//      13   LOAD_VAR 1          (limit)
//      14   BIN_OP *            
//      15   STORE_VAR 2         (result)
//
// 10   16   LOAD_VAR 1          (limit)
//      17   LOAD_CONST 3        (1)
//      18   BIN_OP -            
//      19   STORE_VAR 1         (limit)
//      20   JUMP -19            (to 1)
//      21   POP 1               
//
// 13   22   LOAD_VAR 2          (result)
//      23   RETURN              
//
// Function: Nested
// 17   0    LOAD_CONST 0        (5)
//
// 18   1    LOAD_CONST 1        (true)
//      2    JUMP_IF_FALSE +18   (to 20)
//      3    POP 1               
//
// 19   4    LOAD_CONST 2        (true)
//      5    JUMP_IF_FALSE +8    (to 13)
//      6    POP 1               
//
// 20   7    LOAD_VAR 1          (a)
//      8    LOAD_CONST 3        (1)
//      9    BIN_OP +            
//      10   STORE_VAR 1         (a)
//      11   JUMP +3             (to 14)
//      12   JUMP -8             (to 4)
//      13   POP 1               
//
// 23   14   LOAD_VAR 1          (a)
//      15   LOAD_CONST 4        (1)
//      16   BIN_OP +            
//      17   STORE_VAR 1         (a)
//      18   JUMP +3             (to 21)
//      19   JUMP -18            (to 1)
//      20   POP 1               
//
// 27   21   LOAD_VAR 1          (a)
//      22   RETURN              
//
// Class: std::Request with 3 fields
// Function: len
