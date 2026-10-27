// Decode small JSON orders into nested typed classes: an order holds a
// customer with an address, a shipping address, an enum status and a list of
// line items. Each document is about 300 bytes, so the per-class work of a
// typed decode (choosing how each class decodes) is a large share of the job.
// 1,000 orders are built once in setup(); a job decodes every one with
// `baml.json.decode<Order>` and folds the fields into a checksum.
enum OrderStatus {
  Pending
  Shipped
  Delivered
}

class Address {
  street: string
  city: string
  zip: string
}

class Customer {
  name: string
  email: string
  address: Address
}

class LineItem {
  sku: string
  qty: int
  price_cents: int
}

class Order {
  id: int
  customer: Customer
  shipping: Address
  status: OrderStatus
  items: LineItem[]
}

class Rng {
  seed: int
}

function draw(rng: Rng, n: int) -> int {
  rng.seed = rng.seed * 16807 % 2147483647;
  rng.seed % n
}

function address(rng: Rng) -> string {
  let cities = ["Lisbon", "Oslo", "Lima", "Kyoto", "Austin"];
  "{\"street\":\"" + (1 + draw(rng, 999)).to_string() + " Main St\",\"city\":\"" + cities[draw(rng, 5)]
    + "\",\"zip\":\"" + (10000 + draw(rng, 89999)).to_string() + "\"}"
}

function setup() -> string[] {
  let rng = Rng { seed: 41 };
  let names = ["ada", "bob", "cy", "dee", "eve", "fay"];
  let statuses = ["Pending", "Shipped", "Delivered"];
  let orders: string[] = [];
  for (let i = 0; i < 1000; i += 1) {
    let name = names[draw(rng, 6)];
    let items: string[] = [];
    let count = 1 + draw(rng, 3);
    for (let k = 0; k < count; k += 1) {
      items.push("{\"sku\":\"SKU" + draw(rng, 500).to_string() + "\",\"qty\":" + (1 + draw(rng, 4)).to_string()
        + ",\"price_cents\":" + (99 + draw(rng, 5000)).to_string() + "}");
    }
    let order = "{\"id\":" + i.to_string()
      + ",\"customer\":{\"name\":\"" + name + "\",\"email\":\"" + name + "@example.com\",\"address\":" + address(rng) + "}"
      + ",\"shipping\":" + address(rng)
      + ",\"status\":\"" + statuses[draw(rng, 3)] + "\""
      + ",\"items\":[" + items.join(",") + "]}";
    orders.push(order);
  }
  orders
}

function run(orders: string[]) -> int {
  let check = 0;
  for (let raw in orders) {
    let o = baml.json.decode<Order>(raw);
    let h = o.id + o.customer.name.length() + o.customer.address.city.length() + o.shipping.zip.length();
    h += match (o.status) {
      OrderStatus.Pending => 1,
      OrderStatus.Shipped => 2,
      OrderStatus.Delivered => 3,
    };
    for (let item in o.items) {
      h += item.qty * item.price_cents + item.sku.length();
    }
    check = (check + h) % 1000000007;
  }
  check
}

function main() -> int {
  run(setup())
}
