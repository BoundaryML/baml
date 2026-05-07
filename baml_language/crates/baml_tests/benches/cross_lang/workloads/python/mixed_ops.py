from dataclasses import dataclass


@dataclass
class Point:
    x: int
    y: int


def main() -> int:
    sum_ = 0
    s = ""
    arr: list[int] = []
    for i in range(5000):
        sum_ += i * 3 - 1
        s = s + "x"
        arr.append(i)
        p = Point(x=i, y=i + 1)
        sum_ += p.x + p.y
        if i > 2500:
            sum_ += 1
    return sum_ + len(s) + len(arr)
