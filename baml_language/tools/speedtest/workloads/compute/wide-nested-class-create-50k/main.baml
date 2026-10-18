class Leaf {
  a: int
  b: int
  c: int
  d: int
}
class Node {
  left: Leaf
  right: Leaf
  n0: int
  n1: int
  n2: int
  n3: int
}
class BigRecord {
  id: int
  a0: int
  a1: int
  a2: int
  a3: int
  a4: int
  a5: int
  a6: int
  a7: int
  primary: Node
  secondary: Node
  tail: Leaf
}
function main() -> int {
  let s = 0;
  for (let i = 0; i < 50000; i += 1) {
    let r = BigRecord {
      id: i,
      a0: i + 1,
      a1: i + 2,
      a2: i + 3,
      a3: i + 4,
      a4: i + 5,
      a5: i + 6,
      a6: i + 7,
      a7: i + 8,
      primary: Node {
        left: Leaf { a: i, b: i + 1, c: i + 2, d: i + 3 },
        right: Leaf { a: i + 4, b: i + 5, c: i + 6, d: i + 7 },
        n0: i + 8,
        n1: i + 9,
        n2: i + 10,
        n3: i + 11
      },
      secondary: Node {
        left: Leaf { a: i + 12, b: i + 13, c: i + 14, d: i + 15 },
        right: Leaf { a: i + 16, b: i + 17, c: i + 18, d: i + 19 },
        n0: i + 20,
        n1: i + 21,
        n2: i + 22,
        n3: i + 23
      },
      tail: Leaf { a: i + 24, b: i + 25, c: i + 26, d: i + 27 }
    };
    s += r.id + r.a7 + r.primary.left.c + r.primary.right.d + r.secondary.n3 + r.secondary.right.b + r.tail.d;
  };
  return s;
}
