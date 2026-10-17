def main():
    s = '                              the quick brown fox jumps over the lazy dog and runs through the meadow with great enthusiasm                              '
    t = 0
    for _ in range(100000):
        t += len(s.strip())
    return t
