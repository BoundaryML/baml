interface Animal {
  function noise(self) -> int throws never
}
class Dog {
  implements Animal {
    function noise(self) -> int { return 3 }
  }
}
class Cat {
  implements Animal {
    function noise(self) -> int { return 5 }
  }
}
function pick(i: int) -> Animal {
  if i % 2 == 0 { return Dog {}; };
  return Cat {};
}
function code(a: Animal) -> int {
  return match (a) {
    let d: Dog => d.noise()
    let c: Cat => c.noise()
    _ => 0
  }
}
function main() -> int {
  let acc = 0;
  for (let i = 0; i < 100000; i += 1) {
    acc += code(pick(i));
  };
  return acc;
}
