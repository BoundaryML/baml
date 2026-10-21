
function leaf(n: int) -> int { n }

function main() -> int {
  let s = 0;
  for (let i = 0; i < 1000000; i += 1) { s += leaf(i); };
  return s;
}
