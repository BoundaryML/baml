interface Shape {
  function area(self) -> int throws never
}
class Square {
  side: int
  implements Shape {
    function area(self) -> int { return self.side * self.side }
  }
}
class Rect {
  w: int
  h: int
  implements Shape {
    function area(self) -> int { return self.w * self.h }
  }
}
function pick(i: int) -> Shape {
  if i % 2 == 0 { return Square { side: i }; };
  return Rect { w: i, h: 3 };
}
function area_of(s: Shape) -> int {
  return s.area()
}
function main() -> int {
  let acc = 0;
  for (let i = 0; i < 100000; i += 1) {
    acc += area_of(pick(i));
  };
  return acc;
}
