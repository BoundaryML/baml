// Mint a class with an interface impl on every iteration, the shape of a
// program that builds a dynamic output type per request, then do the
// request's ordinary work on long-lived types: decode a nested typed order,
// compare its status and interpolate its lines. 8 orders are built once in
// setup(); a job runs 200 iterations. Minted classes stay in the engine's
// witness table until a collection sweeps them, and each new class's first
// `Labeled` lookup scans that table, so a job's time grows with the jobs
// before it; bench.toml keeps the job count small.
interface Labeled {
  label: string
  function shout(self) -> string throws never {
    self.label + "!"
  }
}

enum Status {
  Pending
  Shipped
  Delivered
}

class Money {
  cents: int
  implements baml.ToString {
    function to_string(self) -> string throws never {
      "$" + (self.cents / 100).to_string() + "." + (self.cents % 100).to_string()
    }
  }
}

class Address {
  street: string
  city: string
}

class Customer {
  name: string
  address: Address
}

class Item {
  sku: string
  qty: int
  price: Money
}

class Order {
  id: int
  customer: Customer
  status: Status
  items: Item[]
}

function shout_of(value: unknown) -> string {
  match (value) {
    let labeled: Labeled => labeled.shout(),
    _ => "",
  }
}

function setup() -> string[] {
  let cities = ["Lisbon", "Oslo", "Lima", "Kyoto"];
  let statuses = ["Pending", "Shipped", "Delivered"];
  let orders: string[] = [];
  for (let i = 0; i < 8; i += 1) {
    let items: string[] = [];
    for (let k = 0; k < 1 + i % 3; k += 1) {
      items.push("{\"sku\":\"S" + (i * 10 + k).to_string() + "\",\"qty\":" + (1 + k).to_string()
        + ",\"price\":{\"cents\":" + (199 + i * 37 + k).to_string() + "}}");
    }
    orders.push("{\"id\":" + i.to_string() + ",\"customer\":{\"name\":\"c" + i.to_string()
      + "\",\"address\":{\"street\":\"" + (i + 1).to_string() + " Main St\",\"city\":\"" + cities[i % 4] + "\"}}"
      + ",\"status\":\"" + statuses[i % 3] + "\",\"items\":[" + items.join(",") + "]}");
  }
  orders
}

function run(orders: string[]) -> int {
  let total = 0;
  for (let i = 0; i < 200; i += 1) {
    let minted = reflect.Class.new(
      "Row" + i.to_string(),
      { "label": string.type() },
      implementations = [reflect.Interface.implementation<Labeled>().field("label")],
    );
    let type Row = minted;
    let row: Row = baml.json.to<Row>({ "label": "r" + i.to_string() });
    total += shout_of(row).length();
    let order = baml.json.decode<Order>(orders[i % 8]);
    if (order.status == Status.Shipped) { total += 1; }
    for (let item in order.items) {
      total += `${item.qty} x ${item.sku} @ ${item.price}`.length();
    }
    total += `${order.customer.name} in ${order.customer.address.city}`.length() + order.id;
  }
  total
}

function main() -> int {
  run(setup())
}
