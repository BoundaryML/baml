import json


class Rng:
    def __init__(self, seed):
        self.seed = seed

    def draw(self, n):
        self.seed = self.seed * 16807 % 2147483647
        return self.seed % n


def quoted(s):
    return "\"" + s + "\""


def setup():
    rng = Rng(37)
    words = ["alpha", "bravo", "charlie", "delta", "echo", "foxtrot", "golf", "hotel", "india", "juliet", "kilo", "lima"]
    users = []
    for i in range(3200):
        tags = [quoted(words[rng.draw(12)]) for _ in range(1 + rng.draw(4))]
        orders = []
        for _ in range(1 + rng.draw(3)):
            sku = quoted("sku-" + str(rng.draw(100000)))
            price = str(rng.draw(1000)) + "." + str(rng.draw(100))
            orders.append("{\"sku\":" + sku + ",\"qty\":" + str(1 + rng.draw(9)) + ",\"price\":" + price + "}")
        name = words[rng.draw(12)] + "_" + str(i)
        street = str(1 + rng.draw(999)) + " " + words[rng.draw(12)] + " street"
        city = words[rng.draw(12)] + "ville"
        address = "{\"street\":" + quoted(street) + ",\"city\":" + quoted(city) + ",\"zip\":" + quoted(str(10000 + rng.draw(89999))) + "}"
        manager = "null" if rng.draw(3) == 0 else str(rng.draw(3200))
        user = "{\"id\":" + str(i) + ",\"name\":" + quoted(name) + ",\"email\":" + quoted(name + "@example.com")
        user += ",\"age\":" + str(18 + rng.draw(60)) + ",\"active\":" + ("false" if rng.draw(4) == 0 else "true")
        user += ",\"score\":" + str(rng.draw(100)) + "." + str(rng.draw(1000))
        user += ",\"tags\":[" + ",".join(tags) + "],\"address\":" + address + ",\"orders\":[" + ",".join(orders) + "],\"manager_id\":" + manager + "}"
        users.append(user)
    doc = json.loads("{\"meta\":{\"version\":3,\"count\":3200},\"users\":[" + ",".join(users) + "]}")
    return {"doc": doc, "users": doc["users"]}


def path(j, selector):
    # The `.field` and `[index]` steps of a jq-style selector, as baml.json.path reads them.
    current = j
    i = 0
    while i < len(selector):
        if selector[i] == ".":
            i += 1
            start = i
            while i < len(selector) and selector[i] not in ".[":
                i += 1
            if i > start:
                key = selector[start:i]
                if not isinstance(current, dict) or key not in current:
                    raise KeyError(key)
                current = current[key]
        else:
            i += 1
            start = i
            while selector[i] != "]":
                i += 1
            index = int(selector[start:i])
            i += 1
            if not isinstance(current, list) or index >= len(current):
                raise IndexError(index)
            current = current[index]
    return current


def expect(ok, value=None):
    if not ok:
        raise ValueError("does not decode")
    return value


def to_int(v):
    return expect(isinstance(v, int) and not isinstance(v, bool), v)


def to_str(v):
    return expect(isinstance(v, str), v)


def to_float(v):
    return float(expect(isinstance(v, (int, float)) and not isinstance(v, bool), v))


def to_bool(v):
    return expect(isinstance(v, bool), v)


def to_list(v, item):
    expect(isinstance(v, list))
    return [item(x) for x in v]


def to_address(j):
    expect(isinstance(j, dict))
    return {"street": to_str(j["street"]), "city": to_str(j["city"]), "zip": to_str(j["zip"])}


def to_order(j):
    expect(isinstance(j, dict))
    return {"sku": to_str(j["sku"]), "qty": to_int(j["qty"]), "price": to_float(j["price"])}


def to_user(j):
    expect(isinstance(j, dict))
    manager = j.get("manager_id")
    return {
        "id": to_int(j["id"]),
        "name": to_str(j["name"]),
        "email": to_str(j["email"]),
        "age": to_int(j["age"]),
        "active": to_bool(j["active"]),
        "score": to_float(j["score"]),
        "tags": to_list(j["tags"], to_str),
        "address": to_address(j["address"]),
        "orders": to_list(j["orders"], to_order),
        "manager_id": None if manager is None else to_int(manager),
    }


def run(tree):
    rng = Rng(41)
    check = 0
    for _ in range(400):
        base = ".users[" + str(rng.draw(3200)) + "]"
        check += to_int(path(tree["doc"], base + ".age"))
        check += len(to_str(path(tree["doc"], base + ".address.city")))
        check += to_int(path(tree["doc"], base + ".orders[0].qty"))
        manager = path(tree["doc"], base + ".manager_id")
        check += 7 if manager is None else to_int(manager)
        user = to_user(tree["users"][rng.draw(3200)])
        check += len(user["tags"]) * 3 + len(user["orders"]) + len(user["name"]) + (1 if user["active"] else 0)
        check = check % 1000000007
    return check


def main():
    return run(setup())
