class Item { id: int, value: int, next: Item | null, tags: int[] }
class Config { live: int, churn: int, seed: int }
class State { config: Config, items: Item[] }
// The long-lived graph: `live` items, each with a tags array and a pointer
// to its heap-order parent (depth log2(live)), built once and kept for the
// whole process.
function prepare(raw: string) -> State {
    let c = baml.json.deserialize<Config>(raw);
    let items: Item[] = [];
    let i = 0;
    while (i < c.live) {
        let parent: Item | null = null;
        if (i > 0) { parent = items[(i - 1) / 2]; }
        items.push(Item { id: i, value: (i * 7 + c.seed) % 1000, next: parent, tags: [i % 3] });
        i += 1;
    }
    State { config: c, items: items }
}
// `churn` steps over the live graph: replace a pseudo-random item with an
// equal fresh one (old-to-young pointers, the old item lives on while its
// children point at it), allocate a short-lived item pointing at it, and
// walk four hops up the parent chain from there.
function run(s: State) -> string {
    let items = s.items;
    let n = items.length();
    if (n == 0) { return baml.json.to_string([0, 0]); }
    let checksum = 0;
    // A full-period LCG mod 2^31 (the index taken from its high bits): an
    // LCG mod `n` itself can cycle through a few dozen items.
    let state = s.config.seed % 2147483648;
    let x = 0;
    let k = 0;
    while (k < s.config.churn) {
        state = (state * 1103515245 + 12345) % 2147483648;
        x = (state / 256) % n;
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
    baml.json.to_string([n, checksum])
}
