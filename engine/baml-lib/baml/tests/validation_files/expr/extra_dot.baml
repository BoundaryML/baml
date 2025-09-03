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