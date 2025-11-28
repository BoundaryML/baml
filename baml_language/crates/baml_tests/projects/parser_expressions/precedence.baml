function TestPrecedence() -> bool {
  // Multiplication before addition
  let a = 2 + 3 * 4;  // Should be 14, not 20
  let c = 2 * 3 + 4;  // Should be 10, not 14

  // Comparison after arithmetic
  let b = a > 10 + 2;  // Should be a > 12

  // Logical operators
  let d = true || false && false;  // Should be true (AND before OR)

  return d;
}
