def build_array() -> list[int]:
    arr: list[int] = []
    for i in range(10000):
        arr.append(i)
    return arr


def main() -> int:
    arr = build_array()
    s = 0
    for x in arr:
        s += x
    return s
