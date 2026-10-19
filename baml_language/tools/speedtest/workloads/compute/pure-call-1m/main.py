def leaf(n):
    return n

def main():
    s = 0
    for i in range(1000000):
        s += leaf(i)
    return s
