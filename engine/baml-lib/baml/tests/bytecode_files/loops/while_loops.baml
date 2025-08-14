fn GCD(mut a: int, mut b: int) -> int {

	while a != b {
	   if a > b {
	   	a = a - b;
	   } else {
	   	b = b - a;
	   }
	}

	a
}

fn WhileWithScope() -> int {
	
	while true {
		let mut a = 0;

		if a > 0 { continue; }
		if a < 0 { break; }

		// NOTE: there's a compiler bug wrt PopReplace where it will emit PopReplace when the last block's expression.
		// Avoid that by using a statement.
		a = 1;
	}

	3
}

// Function: GCD
// 2    0    LOAD_VAR 1          (a)
//      1    LOAD_VAR 2          (b)
//      2    CMP_OP !=           
//      3    JUMP_IF_FALSE +19   (to 22)
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
//      21   JUMP +2             (to 23)
//      22   POP 1               
//
// 10   23   LOAD_VAR 1          (a)
//      24   RETURN              
//
// Function: WhileWithScope
// 15   0    LOAD_CONST 0        (true)
//      1    JUMP_IF_FALSE +25   (to 26)
//      2    POP 1               
//
// 16   3    LOAD_CONST 1        (0)
//
// 18   4    LOAD_VAR 1          (a)
//      5    LOAD_CONST 2        (0)
//      6    CMP_OP >            
//      7    JUMP_IF_FALSE +4    (to 11)
//      8    POP 1               
//      9    JUMP +13            (to 22)
//      10   JUMP +2             (to 12)
//      11   POP 1               
//
// 19   12   LOAD_VAR 1          (a)
//      13   LOAD_CONST 3        (0)
//      14   CMP_OP <            
//      15   JUMP_IF_FALSE +4    (to 19)
//      16   POP 1               
//      17   JUMP +7             (to 24)
//      18   JUMP +2             (to 20)
//      19   POP 1               
//
// 23   20   LOAD_CONST 4        (1)
//      21   STORE_VAR 1         (a)
//      22   POP 1               
//      23   JUMP -23            (to 0)
//      24   POP 1               
//      25   JUMP +2             (to 27)
//      26   POP 1               
//
// 26   27   LOAD_CONST 5        (3)
//      28   RETURN              
//
// Class: std::Request with 3 fields
// Function: len