class Rng {
  constructor(seed) { this.seed = seed; }
  draw(n) { this.seed = this.seed * 16807 % 2147483647; return this.seed % n; }
}

const quoted = s => "\"" + s + "\"";

function setup() {
  const rng = new Rng(37);
  const words = ["alpha", "bravo", "charlie", "delta", "echo", "foxtrot", "golf", "hotel", "india", "juliet", "kilo", "lima"];
  const users = [];
  for (let i = 0; i < 3200; i++) {
    const tags = [];
    const tagCount = 1 + rng.draw(4);
    for (let t = 0; t < tagCount; t++) tags.push(quoted(words[rng.draw(12)]));
    const orders = [];
    const orderCount = 1 + rng.draw(3);
    for (let o = 0; o < orderCount; o++) {
      const sku = quoted("sku-" + rng.draw(100000));
      const price = rng.draw(1000) + "." + rng.draw(100);
      orders.push("{\"sku\":" + sku + ",\"qty\":" + (1 + rng.draw(9)) + ",\"price\":" + price + "}");
    }
    const name = words[rng.draw(12)] + "_" + i;
    const street = (1 + rng.draw(999)) + " " + words[rng.draw(12)] + " street";
    const address = "{\"street\":" + quoted(street) + ",\"city\":" + quoted(words[rng.draw(12)] + "ville") + ",\"zip\":" + quoted(String(10000 + rng.draw(89999))) + "}";
    const manager = rng.draw(3) === 0 ? "null" : String(rng.draw(3200));
    let user = "{\"id\":" + i + ",\"name\":" + quoted(name) + ",\"email\":" + quoted(name + "@example.com");
    user += ",\"age\":" + (18 + rng.draw(60)) + ",\"active\":" + (rng.draw(4) === 0 ? "false" : "true");
    user += ",\"score\":" + rng.draw(100) + "." + rng.draw(1000);
    user += ",\"tags\":[" + tags.join(",") + "],\"address\":" + address + ",\"orders\":[" + orders.join(",") + "],\"manager_id\":" + manager + "}";
    users.push(user);
  }
  const doc = JSON.parse("{\"meta\":{\"version\":3,\"count\":3200},\"users\":[" + users.join(",") + "]}");
  return { doc, users: doc.users };
}

// The `.field` and `[index]` steps of a jq-style selector, as baml.json.path reads them.
function path(j, selector) {
  let current = j;
  let i = 0;
  while (i < selector.length) {
    if (selector[i] === ".") {
      const start = ++i;
      while (i < selector.length && selector[i] !== "." && selector[i] !== "[") i++;
      if (i > start) {
        const key = selector.slice(start, i);
        if (current === null || typeof current !== "object" || Array.isArray(current) || !(key in current)) {
          throw new Error("missing field " + key);
        }
        current = current[key];
      }
    } else {
      const start = ++i;
      while (selector[i] !== "]") i++;
      const index = Number(selector.slice(start, i++));
      if (!Array.isArray(current) || index >= current.length) throw new Error("bad index " + index);
      current = current[index];
    }
  }
  return current;
}

const isObject = v => v !== null && typeof v === "object" && !Array.isArray(v);

function expect(ok, value) {
  if (!ok) throw new Error("does not decode");
  return value;
}

const toInt = v => expect(Number.isInteger(v), v);
const toString = v => expect(typeof v === "string", v);
const toFloat = v => expect(typeof v === "number", v);
const toBool = v => expect(typeof v === "boolean", v);

function toList(v, item) {
  expect(Array.isArray(v));
  return v.map(item);
}

const toAddress = j => (expect(isObject(j)), { street: toString(j.street), city: toString(j.city), zip: toString(j.zip) });
const toOrder = j => (expect(isObject(j)), { sku: toString(j.sku), qty: toInt(j.qty), price: toFloat(j.price) });

function toUser(j) {
  expect(isObject(j));
  return {
    id: toInt(j.id),
    name: toString(j.name),
    email: toString(j.email),
    age: toInt(j.age),
    active: toBool(j.active),
    score: toFloat(j.score),
    tags: toList(j.tags, toString),
    address: toAddress(j.address),
    orders: toList(j.orders, toOrder),
    manager_id: j.manager_id == null ? null : toInt(j.manager_id),
  };
}

function run(tree) {
  const rng = new Rng(41);
  let check = 0;
  for (let k = 0; k < 400; k++) {
    const base = ".users[" + rng.draw(3200) + "]";
    check += toInt(path(tree.doc, base + ".age"));
    check += toString(path(tree.doc, base + ".address.city")).length;
    check += toInt(path(tree.doc, base + ".orders[0].qty"));
    const manager = path(tree.doc, base + ".manager_id");
    check += manager === null ? 7 : toInt(manager);
    const user = toUser(tree.users[rng.draw(3200)]);
    check += user.tags.length * 3 + user.orders.length + user.name.length + (user.active ? 1 : 0);
    check = check % 1000000007;
  }
  return check;
}

function main() { return run(setup()); }
