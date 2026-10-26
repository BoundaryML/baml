from functools import reduce


class Window:
    __slots__ = ("values", "pos", "end")

    def __init__(self, values, pos, end):
        self.values = values
        self.pos = pos
        self.end = end

    def __iter__(self):
        return self

    def __next__(self):
        if self.pos >= self.end:
            raise StopIteration
        v = self.values[self.pos]
        self.pos += 1
        return v


def main():
    xs = []
    seed = 5
    for _ in range(2000):
        seed = seed * 16807 % 2147483647
        xs.append(seed % 1000)
    acc = 0
    for round in range(10):
        skip = round % 3
        acc += reduce(lambda a, x: a + x, map(lambda x: x * 2 + skip, filter(lambda x: x % 3 != skip, xs)), 0)
        for v in Window(xs, round * 100, round * 100 + 1000):
            acc += v % 17
    return acc
