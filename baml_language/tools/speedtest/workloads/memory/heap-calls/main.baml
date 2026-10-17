// Heap-passing calls (from the native-bench heap_calls case): a loop of small
// functions that take, build and return small objects, through a ring buffer
// that keeps four of them alive.
class Vec2 {
  x: int
  y: int
}

function add(a: Vec2, b: Vec2) -> Vec2 { Vec2 { x: a.x + b.x, y: a.y + b.y } }

function scale(a: Vec2, k: int) -> Vec2 { Vec2 { x: (a.x * k) % 1000003, y: (a.y * k) % 1000003 } }

function pick(a: Vec2, b: Vec2) -> Vec2 { if (a.x <= b.x) { a } else { b } }

function swap(ring: Vec2[], slot: int, v: Vec2) -> Vec2 {
  let old = ring[slot];
  ring[slot] = v;
  old
}

function main() -> string {
  let ring = [Vec2 { x: 0, y: 0 }, Vec2 { x: 1, y: 1 }, Vec2 { x: 2, y: 2 }, Vec2 { x: 3, y: 3 }];
  let acc = Vec2 { x: 7, y: 1 };
  let step = Vec2 { x: 3, y: 5 };
  for (let i = 0; i < 4096; i += 1) {
    let moved = add(acc, step);
    let scaled = scale(moved, 7);
    let old = swap(ring, i % 4, scaled);
    acc = pick(add(old, step), moved);
  }
  acc.x.to_string() + "," + acc.y.to_string() + "," + ring[0].x.to_string() + "," + ring[3].y.to_string()
}
