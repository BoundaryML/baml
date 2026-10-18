def fib(n):
    a, b = (0, 1)
    for _ in range(n):
        a, b = (b, a + b)
    return a

def main():
    s = 0
    for i in range(100000):
        s += fib(50)
    return s
