// Array helpers with lambdas, the ordinary way to process a list: `map`,
// `filter`, `reduce`, `find`, `for_each` and `sort_by` over 4,000 ints and
// 4,000 small class instances, 5 rounds per job. Every lambda captures a
// local of the round.
class Item {
  id: int
  weight: int
}

function main() -> int {
  let xs: int[] = [];
  let items: Item[] = [];
  let seed = 7;
  for (let i = 0; i < 4000; i += 1) {
    seed = seed * 16807 % 2147483647;
    xs.push(seed % 1000);
    items.push(Item { id: i, weight: seed % 97 });
  }
  let acc = 0;
  for (let round = 0; round < 5; round += 1) {
    let k = round + 2;
    let scaled = xs.map((x: int) -> int { x * k + 1 });
    let kept = scaled.filter((x: int) -> bool { x % 3 != 0 });
    acc += kept.reduce((a: int, x: int) -> int { a + x }, 0);
    let hit = xs.find((x: int) -> bool { x == 990 + round });
    acc += hit ?? -1;
    let heavy = items.filter((it: Item) -> bool { it.weight > 40 + round });
    let weights = heavy.map((it: Item) -> int { it.weight * 2 + it.id % 7 });
    let total = 0;
    weights.for_each((w: int) -> void { total += w + round; });
    acc += total;
    let sorted = items.slice(round * 100, round * 100 + 400).sort_by(
      (a: Item, b: Item) -> baml.ops.Ordering {
        if (a.weight < b.weight) {
          baml.ops.Ordering.Less
        } else if (a.weight > b.weight) {
          baml.ops.Ordering.Greater
        } else {
          baml.ops.Ordering.Equal
        }
      }
    );
    acc += sorted[0].id + sorted[399].id * 3;
  }
  acc
}
