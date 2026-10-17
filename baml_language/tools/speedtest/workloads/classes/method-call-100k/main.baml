class Vec {
  x: int
  y: int
  function norm2(self) -> int {
    return self.x * self.x + self.y * self.y
  }
}
function main() -> int {
  let acc = 0;
  for (let i = 0; i < 100000; i += 1) {
    let v = Vec { x: i, y: i + 1 };
    acc += v.norm2();
  };
  return acc;
}
