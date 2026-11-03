import json
from enum import Enum


class Labeled:
    def __init__(self, label):
        self.label = label

    def shout(self):
        return self.label + "!"


class Status(Enum):
    PENDING = "Pending"
    SHIPPED = "Shipped"
    DELIVERED = "Delivered"


def money(cents):
    return "$" + str(cents // 100) + "." + str(cents % 100)


def shout_of(value):
    return value.shout() if isinstance(value, Labeled) else ""


def setup():
    cities = ["Lisbon", "Oslo", "Lima", "Kyoto"]
    statuses = ["Pending", "Shipped", "Delivered"]
    orders = []
    for i in range(8):
        items = []
        for k in range(1 + i % 3):
            items.append('{"sku":"S%d","qty":%d,"price":{"cents":%d}}' % (i * 10 + k, 1 + k, 199 + i * 37 + k))
        orders.append('{"id":%d,"customer":{"name":"c%d","address":{"street":"%d Main St","city":"%s"}},'
                      '"status":"%s","items":[%s]}' % (i, i, i + 1, cities[i % 4], statuses[i % 3], ",".join(items)))
    return orders


def run(orders):
    total = 0
    for i in range(200):
        row_class = type("Row" + str(i), (Labeled,), {})
        row = row_class(json.loads('{"label": "r%d"}' % i)["label"])
        total += len(shout_of(row))
        order = json.loads(orders[i % 8])
        if Status(order["status"]) == Status.SHIPPED:
            total += 1
        for item in order["items"]:
            total += len(f"{item['qty']} x {item['sku']} @ {money(item['price']['cents'])}")
        total += len(f"{order['customer']['name']} in {order['customer']['address']['city']}") + order["id"]
    return total


def main():
    return run(setup())
