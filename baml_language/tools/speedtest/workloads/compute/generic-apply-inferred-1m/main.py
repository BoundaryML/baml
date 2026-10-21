def gapply(f, x):
    return f(x)

def main():
    double = lambda x: x * 2
    s = 0
    for i in range(1000000):
        s += gapply(double, i)
    return s
