class Money:
    __slots__ = ("cents",)

    def __init__(self, cents):
        self.cents = cents

    def __str__(self):
        frac = self.cents % 100
        pad = "0" if frac < 10 else ""
        return "$" + str(self.cents // 100) + "." + pad + str(frac)


class Sku:
    __slots__ = ("family", "number")

    def __init__(self, family, number):
        self.family = family
        self.number = number

    def __str__(self):
        return self.family + "-" + str(self.number)


class Item:
    __slots__ = ("sku", "name", "price", "qty")

    def __init__(self, sku, name, price, qty):
        self.sku = sku
        self.name = name
        self.price = price
        self.qty = qty


def line(item):
    return f"{item.qty} x {item.name} [{item.sku}] @ {item.price}"


def main():
    names = ["apple", "pear", "plum", "fig", "kiwi"]
    families = ["FR", "VG", "DY"]
    seed = 7
    total = 0
    for i in range(10000):
        seed = seed * 16807 % 2147483647
        item = Item(Sku(families[seed % 3], seed % 1000), names[i % 5], Money(seed % 100000), seed % 9 + 1)
        total += len(line(item))
    return total
