function Abs(x: int) -> int {
  if (x < 0) {
    -x
  } else {
    x
  }
}

function Clamp(x: int) -> int {
  let result = if (x < 0) {
    0
  } else if (x > 100) {
    100
  } else {
    x
  };
  result
}
