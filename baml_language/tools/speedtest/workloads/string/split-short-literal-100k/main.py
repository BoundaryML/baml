def main():
    s = 'hello world foo bar baz qux'
    delim = ' '
    c = 0
    for _ in range(100000):
        c += len(s.split(delim))
    return c
