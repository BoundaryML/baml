function fail(status, message) {
  return { status, body: JSON.stringify({ error: message }) };
}
function decodeCreate(text) {
  let v;
  try { v = JSON.parse(text); } catch (e) { return null; }
  if (v === null || typeof v !== "object" || Array.isArray(v)) return null;
  if (typeof v.owner !== "string" || !Number.isInteger(v.balance) || !Array.isArray(v.tags)) return null;
  for (const t of v.tags) if (typeof t !== "string") return null;
  return v;
}
function decodeTransfer(text) {
  let v;
  try { v = JSON.parse(text); } catch (e) { return null; }
  if (v === null || typeof v !== "object" || Array.isArray(v)) return null;
  if (typeof v.source !== "string" || typeof v.target !== "string" || !Number.isInteger(v.amount)) return null;
  return v;
}
class Service {
  constructor() { this.accounts = new Map(); this.nextId = 0; }
}
function createAccount(service, input) {
  service.nextId += 1;
  const id = "acc" + service.nextId;
  service.accounts.set(id, { id, owner: input.owner, balance: input.balance, tags: input.tags });
  return { status: 201, body: JSON.stringify({ id, balance: input.balance }) };
}
function showAccount(account, balanceOnly) {
  if (balanceOnly) return { status: 200, body: JSON.stringify({ balance: account.balance }) };
  return { status: 200, body: JSON.stringify(account) };
}
function moveFunds(source, target, amount) {
  if (amount <= 0) return fail(422, "amount must be positive");
  if (source.balance < amount) return fail(409, "insufficient funds");
  source.balance -= amount;
  target.balance += amount;
  return { status: 200, body: JSON.stringify({ source_balance: source.balance, target_balance: target.balance }) };
}
function transfer(service, input) {
  const source = service.accounts.get(input.source);
  const target = service.accounts.get(input.target);
  if (source === undefined || target === undefined) return fail(404, "no such account");
  return moveFunds(source, target, input.amount);
}
function handle(service, raw) {
  const lines = raw.split("\n");
  const head = lines[0].split(" ");
  if (head.length !== 2) return fail(400, "bad request line");
  const method = head[0];
  const target = head[1].split("?");
  const segments = target[0].split("/");
  const resource = segments[1];
  if (resource === "accounts" && method === "POST" && segments.length === 2 && lines.length === 2) {
    const input = decodeCreate(lines[1]);
    return input === null ? fail(400, "invalid body") : createAccount(service, input);
  }
  if (resource === "accounts" && method === "GET" && segments.length === 3) {
    const account = service.accounts.get(segments[2]);
    if (account === undefined) return fail(404, "no such account");
    return showAccount(account, target.length === 2 && target[1] === "fields=balance");
  }
  if (resource === "accounts") return fail(405, "method not allowed");
  if (resource === "transfer" && method === "POST" && lines.length === 2) {
    const input = decodeTransfer(lines[1]);
    return input === null ? fail(400, "invalid body") : transfer(service, input);
  }
  return fail(404, "no such route");
}
function setup() {
  let seed = 29;
  const draw = (n) => { seed = seed * 16807 % 2147483647; return seed % n; };
  const owners = ["ada", "grace", "linus", "barbara", "ken", "margaret", "dennis", "frances"];
  const requests = [];
  let made = 0;
  for (let i = 0; i < 2000; i++) {
    const kind = draw(10);
    if (kind < 4) {
      const owner = owners[draw(8)];
      const balance = draw(5000);
      const first = draw(5);
      const second = draw(5);
      requests.push('POST /accounts\n{"owner": "' + owner + '", "balance": ' + balance + ', "tags": ["t' + first + '", "t' + second + '"]}');
      made += 1;
    } else if (kind < 7) {
      const id = 1 + draw(made + 3);
      let line = "GET /accounts/acc" + id;
      if (draw(2) === 1) line = line + "?fields=balance";
      requests.push(line);
    } else if (kind < 9) {
      const source = 1 + draw(made + 1);
      const target = 1 + draw(made + 1);
      const amount = draw(3000);
      requests.push('POST /transfer\n{"source": "acc' + source + '", "target": "acc' + target + '", "amount": ' + amount + "}");
    } else {
      const bad = draw(4);
      if (bad === 0) requests.push('POST /accounts\n{"owner": "x"');
      else if (bad === 1) requests.push("GET /nothing");
      else if (bad === 2) requests.push("DELETE /accounts");
      else requests.push('POST /transfer\n{"source": "acc1"}');
    }
  }
  return requests;
}
function run(requests) {
  const service = new Service();
  let check = 0;
  for (const raw of requests) {
    const response = handle(service, raw);
    check = (check * 31 + response.status + response.body.length) % 1000000007;
  }
  let total = 0;
  for (const account of service.accounts.values()) total += account.balance;
  return (check + total + service.nextId) % 1000000007;
}
function main() { return run(setup()); }
