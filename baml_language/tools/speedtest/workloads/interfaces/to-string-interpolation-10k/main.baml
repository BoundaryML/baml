// String interpolation of values whose class overrides `to_string`: the shape
// of rendering log lines or prompt text from typed records. Every line
// interpolates a `Money` (custom `to_string`) and a `Sku` (custom
// `to_string`) beside plain strings and ints. 10,000 lines.
class Money {
  cents: int
  implements baml.ToString {
    function to_string(self) -> string throws never {
      let frac = self.cents % 100;
      let pad = if (frac < 10) { "0" } else { "" };
      "$" + (self.cents / 100).to_string() + "." + pad + frac.to_string()
    }
  }
}

class Sku {
  family: string
  number: int
  implements baml.ToString {
    function to_string(self) -> string throws never {
      self.family + "-" + self.number.to_string()
    }
  }
}

class Item {
  sku: Sku
  name: string
  price: Money
  qty: int
}

function line(item: Item) -> string {
  `${item.qty} x ${item.name} [${item.sku}] @ ${item.price}`
}

function main() -> int {
  let names = ["apple", "pear", "plum", "fig", "kiwi"];
  let families = ["FR", "VG", "DY"];
  let seed = 7;
  let total = 0;
  for (let i = 0; i < 10000; i += 1) {
    seed = seed * 16807 % 2147483647;
    let item = Item {
      sku: Sku { family: families[seed % 3], number: seed % 1000 },
      name: names[i % 5],
      price: Money { cents: seed % 100000 },
      qty: seed % 9 + 1,
    };
    total += line(item).length();
  }
  total
}
