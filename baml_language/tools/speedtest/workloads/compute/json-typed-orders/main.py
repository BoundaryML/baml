import json


class Rng:
    def __init__(self, seed):
        self.seed = seed

    def draw(self, n):
        self.seed = self.seed * 16807 % 2147483647
        return self.seed % n


def address(rng):
    cities = ["Lisbon", "Oslo", "Lima", "Kyoto", "Austin"]
    street = str(1 + rng.draw(999))
    city = cities[rng.draw(5)]
    zip_code = str(10000 + rng.draw(89999))
    return "{\"street\":\"" + street + " Main St\",\"city\":\"" + city + "\",\"zip\":\"" + zip_code + "\"}"


def setup():
    rng = Rng(41)
    names = ["ada", "bob", "cy", "dee", "eve", "fay"]
    statuses = ["Pending", "Shipped", "Delivered"]
    orders = []
    for i in range(1000):
        name = names[rng.draw(6)]
        items = []
        count = 1 + rng.draw(3)
        for _ in range(count):
            sku = str(rng.draw(500))
            qty = str(1 + rng.draw(4))
            price = str(99 + rng.draw(5000))
            items.append("{\"sku\":\"SKU" + sku + "\",\"qty\":" + qty + ",\"price_cents\":" + price + "}")
        customer = "{\"name\":\"" + name + "\",\"email\":\"" + name + "@example.com\",\"address\":" + address(rng) + "}"
        shipping = address(rng)
        status = statuses[rng.draw(3)]
        orders.append("{\"id\":" + str(i) + ",\"customer\":" + customer + ",\"shipping\":" + shipping
                      + ",\"status\":\"" + status + "\",\"items\":[" + ",".join(items) + "]}")
    return orders


STATUS = {"Pending": 1, "Shipped": 2, "Delivered": 3}


def run(orders):
    check = 0
    for raw in orders:
        o = json.loads(raw)
        h = o["id"] + len(o["customer"]["name"]) + len(o["customer"]["address"]["city"]) + len(o["shipping"]["zip"])
        h += STATUS[o["status"]]
        for item in o["items"]:
            h += item["qty"] * item["price_cents"] + len(item["sku"])
        check = (check + h) % 1000000007
    return check


def main():
    return run(setup())
