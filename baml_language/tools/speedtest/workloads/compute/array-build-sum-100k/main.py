def main():
    arr = []
    for i in range(100000):
        arr.append(i)
    s = 0
    for x in arr:
        s += x
    return s
