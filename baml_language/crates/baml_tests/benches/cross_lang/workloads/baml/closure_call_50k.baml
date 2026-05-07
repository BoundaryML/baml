function add_one(n: int) -> int { n + 1 }

function main() -> int {
  let f = add_one;
  let s = 0;
  for (let i = 0; i < 50000; i += 1) {
    s += f(i);
  };
  return s;
}
