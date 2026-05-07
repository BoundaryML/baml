class Point {
  x int
  y int
}

function main() -> int {
  let s = 0;
  for (let i = 0; i < 1000; i += 1) {
    let p = Point { x: i, y: i * 2 };
    s += p.x + p.y;
  };
  return s;
}
