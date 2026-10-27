# A backtick template whose holes are strings: `${a}/${b}`, 10,000 times.
# The same string as `a + "/" + b`.
def main():
    names = ["apple", "pear", "plum", "fig", "kiwi"]
    families = ["FR", "VG", "DY"]
    seed = 7
    total = 0
    for i in range(10000):
        seed = seed * 16807 % 2147483647
        a = names[seed % 5]
        b = families[i % 3]
        s = f"{a}/{b}"
        total += len(s)
    return total
