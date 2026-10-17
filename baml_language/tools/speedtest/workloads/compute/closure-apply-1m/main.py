def apply(f, x):
    return f(x)

def main():
    double = lambda x: x * 2
    s = 0
    for i in range(1000000):
        s += apply(double, i)
    return s
