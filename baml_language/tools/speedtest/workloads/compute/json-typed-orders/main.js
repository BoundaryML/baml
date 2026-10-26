class Rng {
  constructor(seed) { this.seed = seed; }
  draw(n) { this.seed = this.seed * 16807 % 2147483647; return this.seed % n; }
}
function address(rng) {
  const cities = ["Lisbon", "Oslo", "Lima", "Kyoto", "Austin"];
  const street = String(1 + rng.draw(999));
  const city = cities[rng.draw(5)];
  const zip = String(10000 + rng.draw(89999));
  return "{\"street\":\"" + street + " Main St\",\"city\":\"" + city + "\",\"zip\":\"" + zip + "\"}";
}
function setup() {
  const rng = new Rng(41);
  const names = ["ada", "bob", "cy", "dee", "eve", "fay"];
  const statuses = ["Pending", "Shipped", "Delivered"];
  const orders = [];
  for (let i = 0; i < 1000; i++) {
    const name = names[rng.draw(6)];
    const items = [];
    const count = 1 + rng.draw(3);
    for (let k = 0; k < count; k++) {
      const sku = String(rng.draw(500));
      const qty = String(1 + rng.draw(4));
      const price = String(99 + rng.draw(5000));
      items.push("{\"sku\":\"SKU" + sku + "\",\"qty\":" + qty + ",\"price_cents\":" + price + "}");
    }
    const customer = "{\"name\":\"" + name + "\",\"email\":\"" + name + "@example.com\",\"address\":" + address(rng) + "}";
    const shipping = address(rng);
    const status = statuses[rng.draw(3)];
    orders.push("{\"id\":" + String(i) + ",\"customer\":" + customer + ",\"shipping\":" + shipping
      + ",\"status\":\"" + status + "\",\"items\":[" + items.join(",") + "]}");
  }
  return orders;
}
const STATUS = { Pending: 1, Shipped: 2, Delivered: 3 };
function run(orders) {
  let check = 0;
  for (const raw of orders) {
    const o = JSON.parse(raw);
    let h = o.id + o.customer.name.length + o.customer.address.city.length + o.shipping.zip.length;
    h += STATUS[o.status];
    for (const item of o.items) h += item.qty * item.price_cents + item.sku.length;
    check = (check + h) % 1000000007;
  }
  return check;
}
function main() { return run(setup()); }
