class Vec2 { constructor(x, y) { this.x = x; this.y = y; } }
function add(a, b) { return new Vec2(a.x + b.x, a.y + b.y); }
function scale(a, k) { return new Vec2((a.x * k) % 1000003, (a.y * k) % 1000003); }
function pick(a, b) { return a.x <= b.x ? a : b; }
function swap(ring, slot, v) { const old = ring[slot]; ring[slot] = v; return old; }
function main() {
  const ring = [new Vec2(0, 0), new Vec2(1, 1), new Vec2(2, 2), new Vec2(3, 3)];
  let acc = new Vec2(7, 1);
  const step = new Vec2(3, 5);
  for (let i = 0; i < 4096; i++) {
    const moved = add(acc, step);
    const scaled = scale(moved, 7);
    const old = swap(ring, i % 4, scaled);
    acc = pick(add(old, step), moved);
  }
  return `${acc.x},${acc.y},${ring[0].x},${ring[3].y}`;
}
