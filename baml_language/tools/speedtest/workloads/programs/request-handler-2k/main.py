import json


def dumps(value):
    return json.dumps(value, separators=(",", ":"))


def fail(status, message):
    return status, dumps({"error": message})


def decode_create(text):
    try:
        v = json.loads(text)
    except ValueError:
        return None
    if not isinstance(v, dict):
        return None
    owner, balance, tags = v.get("owner"), v.get("balance"), v.get("tags")
    if not isinstance(owner, str) or type(balance) is not int or not isinstance(tags, list):
        return None
    if not all(isinstance(t, str) for t in tags):
        return None
    return v


def decode_transfer(text):
    try:
        v = json.loads(text)
    except ValueError:
        return None
    if not isinstance(v, dict):
        return None
    if not isinstance(v.get("source"), str) or not isinstance(v.get("target"), str) or type(v.get("amount")) is not int:
        return None
    return v


class Service:
    def __init__(self):
        self.accounts = {}
        self.next_id = 0


def create_account(service, inp):
    service.next_id += 1
    id = "acc" + str(service.next_id)
    service.accounts[id] = {"id": id, "owner": inp["owner"], "balance": inp["balance"], "tags": inp["tags"]}
    return 201, dumps({"id": id, "balance": inp["balance"]})


def show_account(account, balance_only):
    if balance_only:
        return 200, dumps({"balance": account["balance"]})
    return 200, dumps(account)


def move_funds(source, target, amount):
    if amount <= 0:
        return fail(422, "amount must be positive")
    if source["balance"] < amount:
        return fail(409, "insufficient funds")
    source["balance"] -= amount
    target["balance"] += amount
    return 200, dumps({"source_balance": source["balance"], "target_balance": target["balance"]})


def transfer(service, inp):
    source = service.accounts.get(inp["source"])
    target = service.accounts.get(inp["target"])
    if source is None or target is None:
        return fail(404, "no such account")
    return move_funds(source, target, inp["amount"])


def handle(service, raw):
    lines = raw.split("\n")
    head = lines[0].split(" ")
    if len(head) != 2:
        return fail(400, "bad request line")
    method = head[0]
    target = head[1].split("?")
    segments = target[0].split("/")
    resource = segments[1]
    if resource == "accounts" and method == "POST" and len(segments) == 2 and len(lines) == 2:
        inp = decode_create(lines[1])
        return fail(400, "invalid body") if inp is None else create_account(service, inp)
    if resource == "accounts" and method == "GET" and len(segments) == 3:
        account = service.accounts.get(segments[2])
        if account is None:
            return fail(404, "no such account")
        return show_account(account, len(target) == 2 and target[1] == "fields=balance")
    if resource == "accounts":
        return fail(405, "method not allowed")
    if resource == "transfer" and method == "POST" and len(lines) == 2:
        inp = decode_transfer(lines[1])
        return fail(400, "invalid body") if inp is None else transfer(service, inp)
    return fail(404, "no such route")


class Rng:
    def __init__(self, seed):
        self.seed = seed

    def draw(self, n):
        self.seed = self.seed * 16807 % 2147483647
        return self.seed % n


def setup():
    rng = Rng(29)
    owners = ["ada", "grace", "linus", "barbara", "ken", "margaret", "dennis", "frances"]
    requests = []
    made = 0
    for _ in range(2000):
        kind = rng.draw(10)
        if kind < 4:
            owner = owners[rng.draw(8)]
            balance = rng.draw(5000)
            first = rng.draw(5)
            second = rng.draw(5)
            requests.append('POST /accounts\n{"owner": "' + owner + '", "balance": ' + str(balance) + ', "tags": ["t' + str(first) + '", "t' + str(second) + '"]}')
            made += 1
        elif kind < 7:
            id = 1 + rng.draw(made + 3)
            line = "GET /accounts/acc" + str(id)
            if rng.draw(2) == 1:
                line = line + "?fields=balance"
            requests.append(line)
        elif kind < 9:
            source = 1 + rng.draw(made + 1)
            target = 1 + rng.draw(made + 1)
            amount = rng.draw(3000)
            requests.append('POST /transfer\n{"source": "acc' + str(source) + '", "target": "acc' + str(target) + '", "amount": ' + str(amount) + "}")
        else:
            bad = rng.draw(4)
            if bad == 0:
                requests.append('POST /accounts\n{"owner": "x"')
            elif bad == 1:
                requests.append("GET /nothing")
            elif bad == 2:
                requests.append("DELETE /accounts")
            else:
                requests.append('POST /transfer\n{"source": "acc1"}')
    return requests


def run(requests):
    service = Service()
    check = 0
    for raw in requests:
        status, body = handle(service, raw)
        check = (check * 31 + status + len(body)) % 1000000007
    total = 0
    for account in service.accounts.values():
        total += account["balance"]
    return (check + total + service.next_id) % 1000000007


def main():
    return run(setup())
