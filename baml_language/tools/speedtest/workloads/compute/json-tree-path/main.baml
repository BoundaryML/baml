// Read typed values out of a large parsed document: a 1 MB JSON array of
// 3,200 user records is parsed once in setup(); a job runs 400 rounds of
// `baml.json.path<T>` reads into it (an int, a string, a nested list element,
// an optional field) and decodes one whole record with `baml.json.to<User>`.
class Address {
  street: string
  city: string
  zip: string
}

class Order {
  sku: string
  qty: int
  price: float
}

class User {
  id: int
  name: string
  email: string
  age: int
  active: bool
  score: float
  tags: string[]
  address: Address
  orders: Order[]
  manager_id: int?
}

class Tree {
  doc: json
  users: json[]
}

class Rng {
  seed: int
}

function draw(rng: Rng, n: int) -> int {
  rng.seed = rng.seed * 16807 % 2147483647;
  rng.seed % n
}

function quoted(s: string) -> string {
  "\"" + s + "\""
}

function setup() -> Tree {
  let rng = Rng { seed: 37 };
  let words = ["alpha", "bravo", "charlie", "delta", "echo", "foxtrot", "golf", "hotel", "india", "juliet", "kilo", "lima"];
  let users: string[] = [];
  for (let i = 0; i < 3200; i += 1) {
    let tags: string[] = [];
    let tag_count = 1 + draw(rng, 4);
    for (let t = 0; t < tag_count; t += 1) { tags.push(quoted(words[draw(rng, 12)])); }
    let orders: string[] = [];
    let order_count = 1 + draw(rng, 3);
    for (let o = 0; o < order_count; o += 1) {
      let sku = quoted("sku-" + draw(rng, 100000).to_string());
      let price = draw(rng, 1000).to_string() + "." + draw(rng, 100).to_string();
      orders.push("{\"sku\":" + sku + ",\"qty\":" + (1 + draw(rng, 9)).to_string() + ",\"price\":" + price + "}");
    }
    let name = words[draw(rng, 12)] + "_" + i.to_string();
    let street = (1 + draw(rng, 999)).to_string() + " " + words[draw(rng, 12)] + " street";
    let address = "{\"street\":" + quoted(street) + ",\"city\":" + quoted(words[draw(rng, 12)] + "ville") + ",\"zip\":" + quoted((10000 + draw(rng, 89999)).to_string()) + "}";
    let manager = if (draw(rng, 3) == 0) { "null" } else { draw(rng, 3200).to_string() };
    let user = "{\"id\":" + i.to_string() + ",\"name\":" + quoted(name) + ",\"email\":" + quoted(name + "@example.com");
    user = user + ",\"age\":" + (18 + draw(rng, 60)).to_string() + ",\"active\":" + (if (draw(rng, 4) == 0) { "false" } else { "true" });
    user = user + ",\"score\":" + draw(rng, 100).to_string() + "." + draw(rng, 1000).to_string();
    user = user + ",\"tags\":[" + tags.join(",") + "],\"address\":" + address + ",\"orders\":[" + orders.join(",") + "],\"manager_id\":" + manager + "}";
    users.push(user);
  }
  let doc = baml.json.parse("{\"meta\":{\"version\":3,\"count\":3200},\"users\":[" + users.join(",") + "]}");
  Tree { doc: doc, users: baml.json.path<json[]>(doc, ".users") }
}

function run(tree: Tree) -> int {
  let rng = Rng { seed: 41 };
  let check = 0;
  for (let k = 0; k < 400; k += 1) {
    let base = ".users[" + draw(rng, 3200).to_string() + "]";
    check += baml.json.path<int>(tree.doc, base + ".age");
    check += baml.json.path<string>(tree.doc, base + ".address.city").length();
    check += baml.json.path<int>(tree.doc, base + ".orders[0].qty");
    check += baml.json.path<int?>(tree.doc, base + ".manager_id") ?? 7;
    let user = baml.json.to<User>(tree.users[draw(rng, 3200)]);
    check += user.tags.length() * 3 + user.orders.length() + user.name.length() + (if (user.active) { 1 } else { 0 });
    check = check % 1000000007;
  }
  check
}

function main() -> int {
  run(setup())
}
