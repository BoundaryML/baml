class Money {
  constructor(cents) { this.cents = cents; }
  toString() {
    const frac = this.cents % 100;
    const pad = frac < 10 ? "0" : "";
    return "$" + String(Math.trunc(this.cents / 100)) + "." + pad + String(frac);
  }
}
class Sku {
  constructor(family, number) { this.family = family; this.number = number; }
  toString() { return this.family + "-" + String(this.number); }
}
class Item {
  constructor(sku, name, price, qty) { this.sku = sku; this.name = name; this.price = price; this.qty = qty; }
}
function line(item) { return `${item.qty} x ${item.name} [${item.sku}] @ ${item.price}`; }
function main() {
  const names = ["apple", "pear", "plum", "fig", "kiwi"];
  const families = ["FR", "VG", "DY"];
  let seed = 7;
  let total = 0;
  for (let i = 0; i < 10000; i++) {
    seed = seed * 16807 % 2147483647;
    const item = new Item(new Sku(families[seed % 3], seed % 1000), names[i % 5], new Money(seed % 100000), seed % 9 + 1);
    total += line(item).length;
  }
  return total;
}
