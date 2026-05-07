def add_one(n: int) -> int:
    return n + 1


def main() -> int:
    f = add_one
    s = 0
    for i in range(50000):
        s += f(i)
    return s
