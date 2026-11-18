// Shared by the cases of the scalar natives: an `int` built at run time.

function count(n: int) -> int {
  let x = 0;
  for (let i = 0; i < n; i += 1) { x += 1; };
  x
}
