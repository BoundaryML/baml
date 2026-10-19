def maybe_divide(a, b):
    if b == 0:
        return 0
    return a // b

def main():
    s = 0
    for i in range(1000000):
        s += maybe_divide(i, i + 1)
    return s
