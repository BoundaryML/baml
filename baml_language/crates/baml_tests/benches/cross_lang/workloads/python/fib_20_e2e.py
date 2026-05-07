def fib(n: int) -> int:
    return n if n <= 1 else fib(n - 1) + fib(n - 2)


def main() -> int:
    return fib(20)
