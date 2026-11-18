// Shared by the cases. Loops keep the optimizer from folding a value into a
// constant, so strings are built on the heap at run time.

function rep(s: string, n: int) -> string {
  let out = "";
  for (let i = 0; i < n; i += 1) { out = `${out}${s}`; };
  out
}

function halves(n: int) -> float {
  let x = 0.0;
  for (let i = 0; i < n; i += 1) { x += 0.5; };
  x
}

function tens(n: int) -> float {
  let x = 1.0;
  for (let i = 0; i < n; i += 1) { x = x * 10.0; };
  x
}
