class Vec2:
    __slots__ = ("x", "y")

    def __init__(self, x, y):
        self.x = x
        self.y = y


def add(a, b):
    return Vec2(a.x + b.x, a.y + b.y)


def scale(a, k):
    return Vec2((a.x * k) % 1000003, (a.y * k) % 1000003)


def pick(a, b):
    return a if a.x <= b.x else b


def swap(ring, slot, v):
    old = ring[slot]
    ring[slot] = v
    return old


def main():
    ring = [Vec2(0, 0), Vec2(1, 1), Vec2(2, 2), Vec2(3, 3)]
    acc = Vec2(7, 1)
    step = Vec2(3, 5)
    for i in range(4096):
        moved = add(acc, step)
        scaled = scale(moved, 7)
        old = swap(ring, i % 4, scaled)
        acc = pick(add(old, step), moved)
    return f"{acc.x},{acc.y},{ring[0].x},{ring[3].y}"
