function FindFirst() -> int {
  let result = 0;
  while (true) {
    if (result == 5) {
      break;
    }
    result = result + 1;
  }
  result
}

function SumOdds() -> int {
  let sum = 0;
  for (let i = 0; i < 10; i += 1) {
    if (i % 2 == 0) {
      continue;
    }
    sum = sum + i;
  }
  sum
}
