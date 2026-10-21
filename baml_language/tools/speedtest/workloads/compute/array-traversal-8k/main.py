def setup():
    seed = 3
    out = []
    for _ in range(8192):
        seed = seed * 16807 % 2147483647
        out.append(seed % 1000)
    return out


def iterate(xs):
    total = 0
    for v in xs:
        total += v
    return total


def indexed(xs):
    total = 0
    i = 0
    while i < len(xs):
        total += xs[i]
        i += 1
    return total


def rebuild(xs):
    ys = []
    j = 0
    while j < len(xs):
        ys.append(xs[j])
        j += 1
    return iterate(ys)


def run(xs):
    return iterate(xs) + indexed(xs) * 3 + rebuild(xs) * 7


def main():
    return run(setup())
