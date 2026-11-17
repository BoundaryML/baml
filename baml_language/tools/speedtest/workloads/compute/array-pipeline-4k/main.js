class Item {
  constructor(id, weight) {
    this.id = id;
    this.weight = weight;
  }
}

function main() {
  const xs = [];
  const items = [];
  let seed = 7;
  for (let i = 0; i < 4000; i++) {
    seed = (seed * 16807) % 2147483647;
    xs.push(seed % 1000);
    items.push(new Item(i, seed % 97));
  }
  let acc = 0;
  for (let round = 0; round < 5; round++) {
    const k = round + 2;
    const scaled = xs.map((x) => x * k + 1);
    const kept = scaled.filter((x) => x % 3 !== 0);
    acc += kept.reduce((a, x) => a + x, 0);
    const hit = xs.find((x) => x === 990 + round);
    acc += hit === undefined ? -1 : hit;
    const heavy = items.filter((it) => it.weight > 40 + round);
    const weights = heavy.map((it) => it.weight * 2 + (it.id % 7));
    let total = 0;
    weights.forEach((w) => {
      total += w + round;
    });
    acc += total;
    const sorted = items
      .slice(round * 100, round * 100 + 400)
      .sort((a, b) => (a.weight < b.weight ? -1 : a.weight > b.weight ? 1 : 0));
    acc += sorted[0].id + sorted[399].id * 3;
  }
  return acc;
}
