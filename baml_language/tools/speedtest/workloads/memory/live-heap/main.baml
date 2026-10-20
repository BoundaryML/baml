// Allocation beside a large live heap (from the native-bench live_heap case):
// setup() builds a long-lived graph of 500,000 items, each with a tags array
// and a pointer to its heap-order parent, once and untimed. Each job takes
// 65,536 steps: replace a pseudo-random item with an equal fresh one
// (old-to-young pointers), allocate a short-lived item pointing at it, and
// walk four hops up the parent chain.
class Item {
  id: int
  value: int
  next: Item | null
  tags: int[]
}

function setup() -> Item[] {
  let items: Item[] = [];
  let i = 0;
  while (i < 500000) {
    let parent: Item | null = null;
    if (i > 0) { parent = items[(i - 1) / 2]; }
    items.push(Item { id: i, value: (i * 7 + 12345) % 1000, next: parent, tags: [i % 3] });
    i += 1;
  }
  items
}

function run(items: Item[]) -> string {
  let n = items.length();
  let checksum = 0;
  let state = 12345;
  let k = 0;
  while (k < 65536) {
    state = state * 48271 % 2147483647;
    let x = state % n;
    let old = items[x];
    let fresh = Item { id: old.id, value: old.value, next: old.next, tags: [old.id % 3] };
    items[x] = fresh;
    let temp = Item { id: k, value: x, next: fresh, tags: [k, x] };
    let cursor: Item | null = temp;
    let hops = 0;
    while (cursor != null && hops < 4) {
      let item: Item = cursor;
      checksum = (checksum + item.value * (hops + 1)) % 1000000007;
      cursor = item.next;
      hops += 1;
    }
    k += 1;
  }
  n.to_string() + "," + checksum.to_string()
}

function main() -> string {
  run(setup())
}
