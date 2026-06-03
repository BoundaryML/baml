function ConstForLoops(items: int[]) -> int {
  const sum = 0;
  for (const i = 0; i < 3; i += 1) {
    sum = sum + i;
  }
  for (const item in items) {
    sum = sum + item;
  }
  sum
}

//----
//- diagnostics
// <no-diagnostics-expected>
