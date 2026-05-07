class Point {
  x int
  y int
  z int
}

function main() -> int {
  let p = Point { x: 1, y: 2, z: 3 };
  let s = 0;
  for (let i = 0; i < 50000; i += 1) {
    s += p.x + p.y + p.z;
  };
  return s;
}
