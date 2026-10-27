class Point {
  constructor(x, y) { this.x = x; this.y = y; }
  static fromJson(j) { return new Point(j[0], j[1]); }
}
class Reading {
  constructor(sensor, value) { this.sensor = sensor; this.value = value; }
  static fromJson(j) { const parts = j.split(":"); return new Reading(parts[0], parseInt(parts[1], 10)); }
}
function parse(T, j) { return T.fromJson(j); }
function main() {
  let seed = 3;
  let acc = 0;
  for (let i = 0; i < 10000; i++) {
    seed = seed * 16807 % 2147483647;
    const p = parse(Point, [seed % 1000, seed % 777]);
    const r = parse(Reading, "s" + String(seed % 10) + ":" + String(seed % 50));
    acc += p.x * 3 + p.y + r.value + r.sensor.length;
  }
  return acc;
}
