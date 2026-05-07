from dataclasses import dataclass


@dataclass
class Point:
    x: int
    y: int


def main() -> int:
    s = 0
    for i in range(50000):
        p = Point(x=i, y=i * 2)
        s += p.x + p.y
    return s
