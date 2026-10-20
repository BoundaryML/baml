def main():
    s = ''
    for _ in range(10000):
        s = s + 'x'
    return len(s)
