class Point:
    __slots__ = ("x", "y")

    def __init__(self, x, y):
        self.x = x
        self.y = y

    @classmethod
    def from_json(cls, j):
        return cls(int(j[0]), int(j[1]))


class Reading:
    __slots__ = ("sensor", "value")

    def __init__(self, sensor, value):
        self.sensor = sensor
        self.value = value

    @classmethod
    def from_json(cls, j):
        parts = j.split(":")
        return cls(parts[0], int(parts[1]))


def parse(t, j):
    return t.from_json(j)


def main():
    seed = 3
    acc = 0
    for i in range(10000):
        seed = seed * 16807 % 2147483647
        p = parse(Point, [seed % 1000, seed % 777])
        r = parse(Reading, "s" + str(seed % 10) + ":" + str(seed % 50))
        acc += p.x * 3 + p.y + r.value + len(r.sensor)
    return acc
