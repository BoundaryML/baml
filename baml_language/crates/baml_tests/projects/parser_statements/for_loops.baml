function c_style_for_loop() -> int {
  let result = 0;
  for (let i = 0; i < 10; i += 1) {
    result += i;
  }
  return result;
}

function iterator_style_for_loop() -> int {
  let result = 0;
  for (let i in [1, 2, 3]) {
    result += i;
  }
  return result;
}

function nested_for_loop() -> int {
  let result = 0;
  for (let i in [1, 2, 3]) {
    for (let j in [4, 5, 6]) {
      result += i * j;
    }
  }
  return result;
}