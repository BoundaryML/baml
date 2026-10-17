interface Counter {
  base: int
  function step(self) -> int throws never
  function total(self) -> int throws never {
    return self.base + self.step()
  }
}
class Inc {
  base: int
  implements Counter {
    function step(self) -> int { return 1 }
  }
}
class Dbl {
  base: int
  implements Counter {
    function step(self) -> int { return self.base }
  }
}
function pick(i: int) -> Counter {
  if i % 2 == 0 { return Inc { base: i }; };
  return Dbl { base: i };
}
function run(c: Counter) -> int {
  return c.total()
}
function main() -> int {
  let acc = 0;
  for (let i = 0; i < 100000; i += 1) {
    acc += run(pick(i));
  };
  return acc;
}
