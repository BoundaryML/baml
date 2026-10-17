class Item { constructor(id, value, next, tags) { this.id = id; this.value = value; this.next = next; this.tags = tags; } }
function setup() {
  const items = [];
  for (let i = 0; i < 500000; i++) {
    const parent = i > 0 ? items[Math.floor((i - 1) / 2)] : null;
    items.push(new Item(i, (i * 7 + 12345) % 1000, parent, [i % 3]));
  }
  return items;
}
function run(items) {
  const n = items.length;
  let checksum = 0, state = 12345;
  for (let k = 0; k < 65536; k++) {
    state = state * 48271 % 2147483647;
    const x = state % n;
    const old = items[x];
    const fresh = new Item(old.id, old.value, old.next, [old.id % 3]);
    items[x] = fresh;
    let cursor = new Item(k, x, fresh, [k, x]);
    let hops = 0;
    while (cursor !== null && hops < 4) {
      checksum = (checksum + cursor.value * (hops + 1)) % 1000000007;
      cursor = cursor.next;
      hops++;
    }
  }
  return `${n},${checksum}`;
}
function main() { return run(setup()); }
