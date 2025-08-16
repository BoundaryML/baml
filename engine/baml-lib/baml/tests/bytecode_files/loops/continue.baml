fn Factorial(mut limit: int) -> int {

	let mut result = 1;


    // used to make the loop break without relying on `break` implementation.
    let mut should_continue = true;
	while should_continue {
		result = result * limit;
		limit = limit - 1;

        if limit != 0 {
            continue;
        } else {
            should_continue = false;
        }

	}

	result
}

fn ContinueNested() -> int {
    while true {
        while false {
            continue;
        }
        if false {
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
//      3    JUMP_IF_FALSE +22   (to 25)
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
//      18   JUMP -16            (to 2)
//      19   JUMP +4             (to 23)
//      20   POP 1               
//
// 14   21   LOAD_CONST 4        (false)
//      22   STORE_VAR 3         (should_continue)
//      23   JUMP -21            (to 2)
//      24   JUMP +2             (to 26)
//      25   POP 1               
//
// 19   26   LOAD_VAR 2          (result)
//      27   RETURN              
//
// Function: ContinueNested
// 23   0    LOAD_CONST 0        (true)
//      1    JUMP_IF_FALSE +18   (to 19)
//      2    POP 1               
//
// 24   3    LOAD_CONST 1        (false)
//      4    JUMP_IF_FALSE +5    (to 9)
//      5    POP 1               
//      6    JUMP -3             (to 3)
//      7    JUMP -4             (to 3)
//      8    JUMP +2             (to 10)
//      9    POP 1               
//
// 27   10   LOAD_CONST 2        (false)
//      11   JUMP_IF_FALSE +4    (to 15)
//      12   POP 1               
//      13   JUMP +2             (to 15)
//      14   JUMP +2             (to 16)
//      15   POP 1               
//      16   JUMP -16            (to 0)
//      17   POP 1               
//      18   JUMP +2             (to 20)
//      19   POP 1               
//
// 31   20   LOAD_CONST 3        (5)
//      21   RETURN              
//
// Class: std::Request with 3 fields
// Function: len