def main() -> int:
    arr: list[int] = []
    for i in range(50000):
        arr.append(i)
    return len(arr)
