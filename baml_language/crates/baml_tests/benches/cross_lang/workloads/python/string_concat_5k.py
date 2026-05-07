def main() -> int:
    s = ""
    for _ in range(5000):
        s = s + "hello"
    return len(s)
