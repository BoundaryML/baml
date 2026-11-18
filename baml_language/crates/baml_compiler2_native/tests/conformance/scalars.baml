// Shared by the cases of the scalar natives: an `int` built at run time.

function count(n: int) -> int {
  let x = 0;
  for (let i = 0; i < n; i += 1) { x += 1; };
  x
}

// Two to the `n`, which past 61 only a 64-bit `int` holds.
function wide(n: int) -> int {
  let x = 1;
  for (let i = 0; i < n; i += 1) { x = x * 2; };
  x
}
