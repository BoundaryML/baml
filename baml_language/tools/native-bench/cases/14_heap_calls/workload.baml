class Vec2 { x: int, y: int }
class State { count: int, seed: int }
function prepare(raw: string) -> State { baml.json.deserialize<State>(raw) }
function add(a: Vec2, b: Vec2) -> Vec2 { Vec2 { x: a.x + b.x, y: a.y + b.y } }
function scale(a: Vec2, k: int) -> Vec2 { Vec2 { x: (a.x * k) % 1000003, y: (a.y * k) % 1000003 } }
function pick(a: Vec2, b: Vec2) -> Vec2 { if (a.x <= b.x) { a } else { b } }
function swap(ring: Vec2[], slot: int, v: Vec2) -> Vec2 {
    let old = ring[slot];
    ring[slot] = v;
    old
}
function run(s: State) -> string {
    let ring = [Vec2 { x: 0, y: 0 }, Vec2 { x: 1, y: 1 }, Vec2 { x: 2, y: 2 }, Vec2 { x: 3, y: 3 }];
    let acc = Vec2 { x: s.seed % 1000, y: 1 };
    let step = Vec2 { x: 3, y: 5 };
    let i = 0;
    while (i < s.count) {
        let moved = add(acc, step);
        let scaled = scale(moved, 7);
        let old = swap(ring, i % 4, scaled);
        acc = pick(add(old, step), moved);
        i += 1;
    }
    baml.json.to_string([acc.x, acc.y, ring[0].x, ring[3].y])
}
