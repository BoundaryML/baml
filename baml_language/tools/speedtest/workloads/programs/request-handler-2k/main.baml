// An API service with the network removed, the shape of an application
// written in BAML: each request is a request line and an optional JSON body.
// The handler splits the line into method, path segments and query, routes
// it, decodes the body into a typed class, validates it, reads and updates
// accounts held in a map, and serializes a typed response. Malformed bodies,
// unknown routes, missing accounts and overdrafts take the error paths.
// 2,000 requests are built once in setup(); a job serves all of them against
// a fresh service and folds every status and body length into a checksum.
class Account { id: string, owner: string, balance: int, tags: string[] }
class CreateAccount { owner: string, balance: int, tags: string[] }
class Transfer { source: string, target: string, amount: int }
class Created { id: string, balance: int }
class Balance { balance: int }
class Moved { source_balance: int, target_balance: int }
class ApiError { error: string }
class Response { status: int, body: string }
class Service {
  accounts: map<string, Account>
  next_id: int
}

function fail(status: int, message: string) -> Response {
  Response { status: status, body: baml.json.stringify(ApiError { error: message }) }
}

function create_account(service: Service, input: CreateAccount) -> Response {
  service.next_id += 1;
  let id = "acc" + service.next_id.to_string();
  service.accounts[id] = Account { id: id, owner: input.owner, balance: input.balance, tags: input.tags };
  Response { status: 201, body: baml.json.stringify(Created { id: id, balance: input.balance }) }
}

function show_account(account: Account, balance_only: bool) -> Response {
  if (balance_only) {
    Response { status: 200, body: baml.json.stringify(Balance { balance: account.balance }) }
  } else {
    Response { status: 200, body: baml.json.stringify(account) }
  }
}

function move_funds(source: Account, target: Account, amount: int) -> Response {
  if (amount <= 0) {
    fail(422, "amount must be positive")
  } else if (source.balance < amount) {
    fail(409, "insufficient funds")
  } else {
    source.balance -= amount;
    target.balance += amount;
    Response { status: 200, body: baml.json.stringify(Moved { source_balance: source.balance, target_balance: target.balance }) }
  }
}

function transfer(service: Service, input: Transfer) -> Response {
  let source = service.accounts.get(input.source);
  let target = service.accounts.get(input.target);
  if (source == null) {
    fail(404, "no such account")
  } else if (target == null) {
    fail(404, "no such account")
  } else {
    move_funds(source, target, input.amount)
  }
}

function handle(service: Service, raw: string) -> Response {
  let lines = raw.split("\n");
  let head = lines[0].split(" ");
  if (head.length() != 2) { return fail(400, "bad request line"); }
  let method = head[0];
  let target = head[1].split("?");
  let segments = target[0].split("/");
  let resource = segments[1];
  if (resource == "accounts" && method == "POST" && segments.length() == 2 && lines.length() == 2) {
    let input: CreateAccount? = baml.json.decode<CreateAccount>(lines[1]) catch (_) { let _ => null };
    if (input == null) { fail(400, "invalid body") } else { create_account(service, input) }
  } else if (resource == "accounts" && method == "GET" && segments.length() == 3) {
    let account = service.accounts.get(segments[2]);
    if (account == null) {
      fail(404, "no such account")
    } else {
      show_account(account, target.length() == 2 && target[1] == "fields=balance")
    }
  } else if (resource == "accounts") {
    fail(405, "method not allowed")
  } else if (resource == "transfer" && method == "POST" && lines.length() == 2) {
    let input: Transfer? = baml.json.decode<Transfer>(lines[1]) catch (_) { let _ => null };
    if (input == null) { fail(400, "invalid body") } else { transfer(service, input) }
  } else {
    fail(404, "no such route")
  }
}

class Rng {
  seed: int
}

function draw(rng: Rng, n: int) -> int {
  rng.seed = rng.seed * 16807 % 2147483647;
  rng.seed % n
}

function setup() -> string[] {
  let rng = Rng { seed: 29 };
  let owners = ["ada", "grace", "linus", "barbara", "ken", "margaret", "dennis", "frances"];
  let requests: string[] = [];
  let made = 0;
  for (let i = 0; i < 2000; i += 1) {
    let kind = draw(rng, 10);
    if (kind < 4) {
      let owner = owners[draw(rng, 8)];
      let balance = draw(rng, 5000);
      let first = draw(rng, 5);
      let second = draw(rng, 5);
      requests.push("POST /accounts\n{\"owner\": \"" + owner + "\", \"balance\": " + balance.to_string() + ", \"tags\": [\"t" + first.to_string() + "\", \"t" + second.to_string() + "\"]}");
      made += 1;
    } else if (kind < 7) {
      let id = 1 + draw(rng, made + 3);
      let line = "GET /accounts/acc" + id.to_string();
      if (draw(rng, 2) == 1) { line = line + "?fields=balance"; }
      requests.push(line);
    } else if (kind < 9) {
      let source = 1 + draw(rng, made + 1);
      let target = 1 + draw(rng, made + 1);
      let amount = draw(rng, 3000);
      requests.push("POST /transfer\n{\"source\": \"acc" + source.to_string() + "\", \"target\": \"acc" + target.to_string() + "\", \"amount\": " + amount.to_string() + "}");
    } else {
      let bad = draw(rng, 4);
      if (bad == 0) {
        requests.push("POST /accounts\n{\"owner\": \"x\"");
      } else if (bad == 1) {
        requests.push("GET /nothing");
      } else if (bad == 2) {
        requests.push("DELETE /accounts");
      } else {
        requests.push("POST /transfer\n{\"source\": \"acc1\"}");
      }
    }
  }
  requests
}

function run(requests: string[]) -> int {
  let service = Service { accounts: {}, next_id: 0 };
  let check = 0;
  for (let raw in requests) {
    let response = handle(service, raw);
    check = (check * 31 + response.status + response.body.length()) % 1000000007;
  }
  let total = 0;
  for (let account in service.accounts.values()) { total += account.balance; }
  (check + total + service.next_id) % 1000000007
}

function main() -> int {
  run(setup())
}
