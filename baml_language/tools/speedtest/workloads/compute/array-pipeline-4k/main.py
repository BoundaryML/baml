import functools


class Item:
    __slots__ = ("id", "weight")

    def __init__(self, id, weight):
        self.id = id
        self.weight = weight


def main():
    xs = []
    items = []
    seed = 7
    for i in range(4000):
        seed = seed * 16807 % 2147483647
        xs.append(seed % 1000)
        items.append(Item(i, seed % 97))
    acc = 0
    for round in range(5):
        k = round + 2
        scaled = list(map(lambda x: x * k + 1, xs))
        kept = list(filter(lambda x: x % 3 != 0, scaled))
        acc += functools.reduce(lambda a, x: a + x, kept, 0)
        hit = next((x for x in xs if x == 990 + round), None)
        acc += hit if hit is not None else -1
        heavy = list(filter(lambda it: it.weight > 40 + round, items))
        weights = list(map(lambda it: it.weight * 2 + it.id % 7, heavy))
        total = 0
        for w in weights:
            total += w + round
        acc += total
        compare = lambda a, b: -1 if a.weight < b.weight else (1 if a.weight > b.weight else 0)
        part = sorted(items[round * 100 : round * 100 + 400], key=functools.cmp_to_key(compare))
        acc += part[0].id + part[399].id * 3
    return acc
