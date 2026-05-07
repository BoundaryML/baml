from dataclasses import dataclass


@dataclass
class Point:
    x: int
    y: int
    z: int


def main() -> int:
    p = Point(x=1, y=2, z=3)
    s = 0
    for _ in range(50000):
        s += p.x + p.y + p.z
    return s
