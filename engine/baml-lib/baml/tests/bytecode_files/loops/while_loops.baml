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
// Class: std::Request with 3 fields