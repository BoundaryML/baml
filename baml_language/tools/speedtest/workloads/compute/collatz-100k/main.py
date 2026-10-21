def collatz_steps(n):
    steps = 0
    x = n
    while x != 1:
        if x % 2 == 0:
            x = x // 2
        else:
            x = 3 * x + 1
        steps += 1
    return steps

def main():
    s = 0
    for i in range(1, 100000):
        s += collatz_steps(i)
    return s
