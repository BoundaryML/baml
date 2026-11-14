class Labeled {
  constructor(label) { this.label = label; }
  shout() { return this.label + "!"; }
}
function money(cents) { return "$" + String(Math.trunc(cents / 100)) + "." + String(cents % 100); }
function shoutOf(value) { return value instanceof Labeled ? value.shout() : ""; }
function setup() {
  const cities = ["Lisbon", "Oslo", "Lima", "Kyoto"];
  const statuses = ["Pending", "Shipped", "Delivered"];
  const orders = [];
  for (let i = 0; i < 8; i++) {
    const items = [];
    for (let k = 0; k < 1 + i % 3; k++) {
      items.push('{"sku":"S' + (i * 10 + k) + '","qty":' + (1 + k) + ',"price":{"cents":' + (199 + i * 37 + k) + '}}');
    }
    orders.push('{"id":' + i + ',"customer":{"name":"c' + i + '","address":{"street":"' + (i + 1) + ' Main St","city":"'
      + cities[i % 4] + '"}},"status":"' + statuses[i % 3] + '","items":[' + items.join(",") + ']}');
  }
  return orders;
}
function run(orders) {
  let total = 0;
  for (let i = 0; i < 200; i++) {
    const Row = { ["Row" + i]: class extends Labeled {} }["Row" + i];
    const row = new Row(JSON.parse('{"label": "r' + i + '"}').label);
    total += shoutOf(row).length;
    const order = JSON.parse(orders[i % 8]);
    if (order.status === "Shipped") total += 1;
    for (const item of order.items) total += `${item.qty} x ${item.sku} @ ${money(item.price.cents)}`.length;
    total += `${order.customer.name} in ${order.customer.address.city}`.length + order.id;
  }
  return total;
}
function main() { return run(setup()); }
