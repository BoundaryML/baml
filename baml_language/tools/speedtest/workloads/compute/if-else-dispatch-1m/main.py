def classify(x):
    r = x % 5
    if r == 0:
        return 1
    elif r == 1:
        return 2
    elif r == 2:
        return 3
    elif r == 3:
        return 4
    else:
        return 5

def main():
    s = 0
    for i in range(1000000):
        s += classify(i)
    return s
