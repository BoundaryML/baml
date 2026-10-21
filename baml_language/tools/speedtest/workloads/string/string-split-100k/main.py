def main():
    c = 0
    for i in range(100000):
        s = 'hello world foo bar baz qux'
        c += len(s.split(' '))
    return c
