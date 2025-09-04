class Receipt {
  establishment_name string
  total float
  currency string

  function calculate(self) -> float {
    self.total
  }
}

function ModernFunction(x: int) -> int {
 
  // Object construction with fields
  let receipt = Receipt {
    establishment_name: "Pizzaria la Rocca",
    total: 25.50,
    currency: "USD",
  };
   
  // Method call
  .let result = receipt.calculate();

  ^if (true) {
    return 1;
  }
   
  // Assignment operators
  result += 5.0;
  result -= 2.0;
  result *= 3.0;
  result /= 4.0;
   
  // Binary operators
  let result = 100;
  if (result > 10 && result < 100) {
    result
  } else {
    0
  }
}

// error: Invalid statement
//   -->  expr/extra_dot.baml:21
//    | 
// 20 |   // Method call
// 21 |   .let result = receipt.calculate();
// 22 | 
//    | 
// error: Invalid statement
//   -->  expr/extra_dot.baml:23
//    | 
// 22 | 
// 23 |   ^if (true) {
// 24 |     return 1;
// 25 |   }
// 26 |    
//    | 
